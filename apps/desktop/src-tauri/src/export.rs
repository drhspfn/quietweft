//! Save the exact original/processed WAV bytes through a native file dialog.
use base64::{engine::general_purpose::STANDARD,Engine as _};
use std::{io::Write,path::{Path,PathBuf}};
use tauri_plugin_dialog::DialogExt;

fn wav_bytes(audio:&str)->Result<Vec<u8>,String>{
    crate::benchmark::decode_audio_uri(audio)?;
    STANDARD.decode(audio.strip_prefix("data:audio/wav;base64,").ok_or("Expected WAV audio")?)
        .map_err(|_|"Invalid WAV encoding".into())
}

fn write_wav(path:&Path,bytes:&[u8])->Result<PathBuf,String>{
    let mut path=path.to_path_buf();
    if path.extension().is_none(){
        path.set_extension("wav");
        if path.exists(){return Err("That WAV already exists. Select it in the save dialog to replace it.".into());}
    }
    if !path.extension().is_some_and(|ext|ext.eq_ignore_ascii_case("wav")){
        return Err("Choose a filename ending in .wav".into());
    }
    let nonce=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e|e.to_string())?.as_nanos();
    let temporary=path.with_file_name(format!(".cvl-export-{}-{nonce}.tmp",std::process::id()));
    let result=(||->std::io::Result<()>{
        let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(&temporary)?;
        file.write_all(bytes)?;file.sync_all()?;drop(file);
        std::fs::rename(&temporary,&path)
    })();
    if let Err(error)=result{
        let _=std::fs::remove_file(&temporary);
        return Err(format!("Could not save WAV: {error}"));
    }
    Ok(path)
}

#[tauri::command]
pub async fn save_wav(window:tauri::WebviewWindow,audio:String,track:String)->Result<Option<String>,String>{
    let filename=match track.as_str(){
        "before"=>"causal-voice-original.wav",
        "after"=>"causal-voice-processed.wav",
        _=>return Err("Unknown audio track".into()),
    };
    let bytes=wav_bytes(&audio)?;
    tauri::async_runtime::spawn_blocking(move ||{
        let choice=window.dialog().file().set_parent(&window).set_file_name(filename)
            .add_filter("WAV audio",&["wav"]).blocking_save_file();
        let Some(choice)=choice else{return Ok(None)};
        let path=choice.into_path().map_err(|e|e.to_string())?;
        Ok(Some(write_wav(&path,&bytes)?.to_string_lossy().to_string()))
    }).await.map_err(|e|e.to_string())?
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn exports_original_and_processed_wavs_without_changing_samples(){
        let root=std::env::temp_dir().join(format!("cvl-export-test-{}",std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        for (name,samples) in [("original.wav",vec![0.2,-0.3,0.1]),("processed.wav",vec![0.02,-0.03,0.01])]{
            let uri=crate::benchmark::audio_uri(&samples).unwrap();
            let bytes=wav_bytes(&uri).unwrap();let path=root.join(name);
            write_wav(&path,&bytes).unwrap();
            assert_eq!(std::fs::read(&path).unwrap(),bytes);
            // Replacement uses the same atomic path as an overwrite confirmed in the dialog.
            write_wav(&path,&bytes).unwrap();
            assert_eq!(crate::benchmark::decode_audio_uri(&uri).unwrap().len(),samples.len());
            std::fs::remove_file(path).unwrap();
        }
        assert!(wav_bytes("not audio").is_err());
        assert!(write_wav(&root.join("wrong.mp3"),b"bad").is_err());
        std::fs::remove_dir(root).unwrap();
    }
}
