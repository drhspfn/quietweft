//! Safe-ish private owner around the C ABI exported by voice_runtime.dll.
//! Handles are NEVER shared between the UI and an inference thread.
use std::{ffi::{CStr, CString, c_char, c_void},path::{Path,PathBuf}};
use libloading::Library;

#[repr(C)]
#[derive(Clone,Copy,Debug,Default)]
pub struct Stats {
    pub processed_hops:u64,
    pub processed_samples:u64,
    pub last_us:f64,
    pub mean_us:f64,
    pub peak_us:f64,
    pub realtime_factor:f64,
    pub algorithmic_latency_ms:f64,
    pub last_error_code:i32,
}

type Create = unsafe extern "C" fn(*const c_char)->*mut c_void;
type Destroy = unsafe extern "C" fn(*mut c_void);
type Version = unsafe extern "C" fn()->u32;
type CreateError = unsafe extern "C" fn()->*const c_char;
type Process = unsafe extern "C" fn(*mut c_void,*const f32,*mut f32)->i32;
type Strength = unsafe extern "C" fn(*mut c_void,u32,f32)->i32;
type GetStats = unsafe extern "C" fn(*const c_void,*mut Stats)->i32;
type LastError = unsafe extern "C" fn(*const c_void)->*const c_char;

pub struct NativeEngine {
    _library:Library,
    handle:*mut c_void,
    destroy:Destroy,
    process:Process,
    set_strength:Strength,
    get_stats:GetStats,
    last_error:LastError,
}

impl NativeEngine {
    pub fn load(dir:&Path)->Result<Self,String>{
        let dll=find_dll()?;
        // SAFETY: library symbols and their signatures match rust/voice-runtime/src/lib.rs.
        let library=unsafe{Library::new(&dll)}.map_err(|e|format!("DLL {}: {e}",dll.display()))?;
        unsafe {
            macro_rules! sym {($name:literal, $kind:ty)=>{{
                *library.get::<$kind>(concat!($name,"\0").as_bytes())
                    .map_err(|e|format!("DLL missing {}: {e}",$name))?
            }}}
            let version:Version=sym!("cvl_abi_version",Version);
            if version()!=1{return Err(format!("Unsupported DLL ABI {} (requires 1)",version()));}
            let create:Create=sym!("cvl_create",Create);
            let destroy:Destroy=sym!("cvl_destroy",Destroy);
            let err:CreateError=sym!("cvl_last_create_error",CreateError);
            let process:Process=sym!("cvl_process_hop",Process);
            let set_strength:Strength=sym!("cvl_set_strength",Strength);
            let get_stats:GetStats=sym!("cvl_get_stats",GetStats);
            let last_error:LastError=sym!("cvl_last_error",LastError);
            let path=CString::new(dir.to_string_lossy().as_bytes()).map_err(|e|e.to_string())?;
            let handle=create(path.as_ptr());
            if handle.is_null(){
                let ptr=err();
                return Err(if ptr.is_null(){"Failed to create model context".into()}
                           else{CStr::from_ptr(ptr).to_string_lossy().into_owned()});
            }
            Ok(Self{_library:library,handle,destroy,process,set_strength,get_stats,last_error})
        }
    }
    pub fn set_all(&mut self,gains:&[f32])->Result<(),String>{
        for (index,value) in gains.iter().enumerate(){
            if !(0.0..=1.0).contains(value) || !value.is_finite(){return Err("Strength must be between 0 and 1".into());}
            let result=unsafe{(self.set_strength)(self.handle,index as u32,*value)};
            if result!=0{return Err(format!("set_strength({index}) returned {result}"));}
        }
        Ok(())
    }
    pub fn process(&mut self,input:&[f32;128])->Result<[f32;128],String>{
        let mut output=[0.0f32;128];
        let result=unsafe{(self.process)(self.handle,input.as_ptr(),output.as_mut_ptr())};
        if result!=0{
            let p=unsafe{(self.last_error)(self.handle)};
            let message=if p.is_null(){format!("Inference error {result}")}
                        else{unsafe{CStr::from_ptr(p).to_string_lossy().into_owned()}};
            return Err(message);
        }
        Ok(output)
    }
    pub fn stats(&self)->Stats {
        let mut stats=Stats::default();
        let _=unsafe{(self.get_stats)(self.handle,&mut stats)};
        stats
    }
}
impl Drop for NativeEngine {
    fn drop(&mut self){unsafe{(self.destroy)(self.handle)}}
}

fn find_dll()->Result<PathBuf,String>{
    let exe=std::env::current_exe().map_err(|e|e.to_string())?;
    let mut candidates=Vec::new();
    if let Some(p)=exe.parent(){candidates.push(p.join(dll_filename()));}
    if let Ok(cwd)=std::env::current_dir(){
        candidates.push(cwd.join(dll_filename()));
        candidates.push(cwd.join("rust/target/release").join(dll_filename()));
        candidates.push(cwd.join("rust/target/debug").join(dll_filename()));
    }
    // Development from apps/desktop/src-tauri, independent of user's cwd.
    let root=Path::new(env!("CARGO_MANIFEST_DIR"));
    candidates.push(root.join("../../../rust/target/release").join(dll_filename()));
    candidates.push(root.join("../../../rust/target/debug").join(dll_filename()));
    candidates.into_iter().find(|p|p.is_file()).ok_or_else(||
        format!("{} not found. Run scripts/build.ps1 -Platform windows to build/package it.",dll_filename()))
}
#[cfg(target_os="windows")]
fn dll_filename()->&'static str{"voice_runtime.dll"}
#[cfg(target_os="macos")]
fn dll_filename()->&'static str{"libvoice_runtime.dylib"}
#[cfg(all(not(target_os="windows"),not(target_os="macos")))]
fn dll_filename()->&'static str{"libvoice_runtime.so"}
