import { useEffect, useRef, useState } from 'react'
import { invoke, isTauri } from '@tauri-apps/api/core'
import { ArrowDown, AudioLines, Headphones, Mic, Power, RefreshCw, Square, Waypoints } from 'lucide-react'
import './styles.css'

type Device = {id:string; name:string; default:boolean; virtualCable:boolean}
type Devices = {inputs:Device[]; outputs:Device[]}
type Metrics = {active:boolean; phase:string; error:string; inputLevel:number; outputLevel:number}
const idle:Metrics = {active:false,phase:'idle',error:'',inputLevel:0,outputLevel:0}
const message = (error:unknown) => String(error).replace(/^Error: /,'')

function Level({label,value,active,source=false}:{label:string;value:number;active:boolean;source?:boolean}) {
  const db = 20*Math.log10(Math.max(value,0.000001))
  const percent = active ? Math.max(0,Math.min(100,(db+60)/60*100)) : 0
  return <div className={`level ${source?'source':''}`}><div className="level-label"><span>{label}</span><span>{active&&db>-60?`${db.toFixed(0)} dBFS`:'—'}</span></div><div className="meter" role="meter" aria-label={label} aria-valuemin={-60} aria-valuemax={0} aria-valuenow={active?Math.max(-60,Math.min(0,db)):-60}><div style={{width:`${percent}%`}}/></div></div>
}

export default function App() {
  const [devices,setDevices]=useState<Devices>({inputs:[],outputs:[]})
  const [input,setInput]=useState(''), [output,setOutput]=useState('')
  const [route,setRoute]=useState<'call'|'listen'>('call')
  const [enabled,setEnabled]=useState(true), [metrics,setMetrics]=useState(idle)
  const [busy,setBusy]=useState(false), [loading,setLoading]=useState(true), [error,setError]=useState('')
  const mounted=useRef(true), polling=useRef(false)
  const active=metrics.active
  const outputs=devices.outputs.filter(d=>route==='listen'?!d.virtualCable:d.virtualCable)
  const locked=busy||active

  async function refresh() {
    setLoading(true); setError('')
    try {
      if (!isTauri()) throw new Error('Open the installed Quietweft app to access audio devices.')
      const next=await invoke<Devices>('list_audio_devices')
      if(!mounted.current)return
      setDevices(next)
      setInput(current=>next.inputs.some(d=>d.id===current)?current:(next.inputs.find(d=>d.default)?.id??next.inputs[0]?.id??''))
      setOutput(current=>next.outputs.some(d=>d.id===current)?current:'')
    } catch(e) {if(mounted.current)setError(message(e))}
    finally {if(mounted.current)setLoading(false)}
  }

  useEffect(()=>{mounted.current=true;void refresh();return()=>{mounted.current=false}},[])
  useEffect(()=>{
    if(!outputs.some(d=>d.id===output))setOutput(outputs.find(d=>d.default)?.id??outputs[0]?.id??'')
  },[devices,route])
  useEffect(()=>{
    if(!active)return
    const timer=setInterval(async()=>{
      if(polling.current)return
      polling.current=true
      try {const status=await invoke<Metrics>('live_status');if(mounted.current){setMetrics(status);if(status.error)setError(status.error)}}
      catch(e){if(mounted.current)setError(message(e))}
      finally{polling.current=false}
    },120)
    return()=>clearInterval(timer)
  },[active])

  async function toggleSession() {
    if(busy)return
    setBusy(true);setError('')
    try {
      if(active){await invoke('stop_live');setMetrics(idle)}
      else setMetrics(await invoke<Metrics>('start_live',{request:{inputId:input,outputId:output,enabled}}))
    }catch(e){setError(message(e))}
    finally{setBusy(false)}
  }
  async function toggleSuppression() {
    if(busy)return
    const next=!enabled
    setBusy(true);setError('')
    try {if(active)await invoke('set_enabled',{enabled:next});setEnabled(next)}
    catch(e){setError(message(e))}
    finally{setBusy(false)}
  }

  return <main>
    <header><div className="brand"><img src="./brand/mark.png" width="44" height="44" alt=""/><div><h1>Quietweft</h1><p>A quieter microphone.</p></div></div><span className="version">1.4.4</span></header>
    <section className="routing" aria-labelledby="route-title">
      <div className="section-heading"><h2 id="route-title">Your audio route</h2><button className="refresh" disabled={locked||loading} onClick={()=>void refresh()} aria-label="Refresh audio devices" title="Refresh audio devices"><RefreshCw size={15} className={loading?'spin':''}/></button></div>
      <label htmlFor="input"><Mic size={15}/> Microphone</label>
      <select id="input" value={input} onChange={e=>setInput(e.target.value)} disabled={locked||loading||!devices.inputs.length}><option value="" disabled>{loading?'Finding microphones…':'No microphones found'}</option>{devices.inputs.map(d=><option key={d.id} value={d.id}>{d.name}</option>)}</select>
      <div className="route-divider"><span/><ArrowDown size={15}/><span/></div>
      <div className="destination-tabs" role="group" aria-label="Output destination"><button aria-pressed={route==='call'} disabled={locked} onClick={()=>setRoute('call')}><Waypoints size={15}/> Send to a call</button><button aria-pressed={route==='listen'} disabled={locked} onClick={()=>setRoute('listen')}><Headphones size={15}/> Listen in headphones</button></div>
      <label className="sr-only" htmlFor="output">Output device</label>
      <select id="output" aria-describedby="route-help" value={output} onChange={e=>setOutput(e.target.value)} disabled={locked||loading||!outputs.length}><option value="" disabled>{loading?'Finding outputs…':route==='call'?'No virtual audio device found':'No output devices found'}</option>{outputs.map(d=><option key={d.id} value={d.id}>{d.name}</option>)}</select>
      <p id="route-help" className="help">{route==='listen'?'Use headphones to avoid microphone feedback.':outputs.length?'In your call app, select the matching virtual microphone.':'Install a virtual audio device, then refresh. Or listen in headphones.'}</p>
    </section>
    <section className={`processing ${enabled?'enabled':''}`} aria-label="Noise suppression"><div className="processing-icon"><AudioLines size={22}/></div><div><h2>Noise suppression</h2><p>{enabled?'Keep your voice in focus.':'Original microphone audio.'}</p></div><button className="switch" role="switch" aria-checked={enabled} aria-label="Noise suppression" disabled={busy} onClick={()=>void toggleSuppression()}><span className="switch-knob"/><span>{enabled?'On':'Off'}</span></button></section>
    <section className="levels" aria-label="Live audio levels"><Level label="Microphone" value={metrics.inputLevel} active={active} source/><Level label="Output" value={metrics.outputLevel} active={active}/></section>
    <div className="session-status" role="status" aria-live="polite"><span className={`status-dot ${active?'live':''}`}/>{busy?active?'Stopping…':'Updating audio…':active?enabled?'Noise suppression is running':'Microphone is running · suppression off':'Microphone is off'}</div>
    <button className={`primary ${active?'running':''}`} disabled={busy||loading||(!active&&(!input||!output))} aria-busy={busy} onClick={()=>void toggleSession()}>{active?<Square size={17}/>:<Power size={19}/>}<span>{busy?active?'Stopping microphone…':'Please wait…':active?'Stop microphone':'Start microphone'}</span></button>
    <div className="feedback" role={error?'alert':undefined}>{error||(!active?'Audio stays on this computer. Nothing is recorded.':'Stop the microphone to change devices.')}</div>
  </main>
}
