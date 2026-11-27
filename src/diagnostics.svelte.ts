import {invoke as nativeInvoke} from '@tauri-apps/api/core';

import * as dialogs from '@tauri-apps/plugin-dialog';

type Level='info'|'warning'|'error';
type Entry={time:string;level:Level;context:string;message:string;session?:string};
const key='studio-editor-errors';
const session=crypto.randomUUID();
function startSession():Entry[]{
  try{
    const data:unknown=JSON.parse(localStorage.getItem(key)??'[]');
    const entries=Array.isArray(data)?data:(data as {entries?:Entry[]}|null)?.entries;
    const lastSession=Array.isArray(data)?data.at(-1)?.session:(data as {session?:string}|null)?.session;
    localStorage.setItem(key,JSON.stringify({session,entries:[]}));
    return lastSession && Array.isArray(entries)?entries.filter(e=>e?.session===lastSession &&
      typeof e.time==='string' && typeof e.context==='string' && typeof e.message==='string').slice(-250):[];
  }catch{return [];}
}
export const diagnostics=$state({entries:[] as Entry[],history:startSession(),unreadErrors:0});
export function recordActivity(context:string,message:string,level:Level='info'){
  const entry={session,time:new Date().toISOString(),level,context,message:message.slice(0,8000)};
  if(level==='error')diagnostics.unreadErrors++;
  diagnostics.entries=[...diagnostics.entries.slice(-249),entry];
  try{
    // Only the most recently opened window supplies the log for the next launch.
    if(JSON.parse(localStorage.getItem(key)??'null')?.session===session)
      localStorage.setItem(key,JSON.stringify({session,entries:diagnostics.entries}));
  }catch{/* Keep the in-memory log if storage is full. */}
}

export function recordError(context:string,error:unknown){
  const message=error instanceof Error?error.stack??error.message:String(error);
  const last=diagnostics.entries.at(-1);
  if(last?.level==='error' && last.message===message && last.context!==context &&
    Date.now()-Date.parse(last.time)<1000)return;
  recordActivity(context,message,'error');
}
export async function invoke<T>(command:string,args?:Record<string,unknown>):Promise<T>{
  try{return await nativeInvoke<T>(command,args);}
  catch(error){recordError(command,error);throw error;}
}
function logCall<Args extends unknown[],Result>(context:string,call:(...args:Args)=>Promise<Result>){
  return async (...args:Args)=>{
    try{return await call(...args);}
    catch(error){recordError(context,error);throw error;}
  };
}
export const open=logCall('Open file dialog',dialogs.open);
export const save=logCall('Save file dialog',dialogs.save);
export const confirm=logCall('Confirmation dialog',dialogs.confirm);

export async function collectNativeDiagnostics(){
  try{
    const entries=await nativeInvoke<{context:string;message:string}[]>('take_native_diagnostics');
    for(const {context,message} of entries)recordError(context,message);
  }catch(error){recordError('Native diagnostics',error);throw error;}
}

window.addEventListener('error',(e:Event)=>{
  if(e instanceof ErrorEvent)recordError('Editor',e.error??e.message);
  else if(e.target instanceof Element && !(e.target instanceof HTMLMediaElement))
    recordError('Editor resource',e.target.tagName.toLowerCase()+' could not be loaded.');
},true);
window.addEventListener('unhandledrejection',e=>recordError('Unhandled promise',e.reason));
