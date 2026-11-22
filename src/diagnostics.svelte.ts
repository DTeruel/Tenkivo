import {invoke as nativeInvoke} from '@tauri-apps/api/core';

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
export const diagnostics=$state({entries:[] as Entry[],history:startSession()});
export function recordActivity(context:string,message:string,level:Level='info'){
  const entry={session,time:new Date().toISOString(),level,context,message:message.slice(0,8000)};
  diagnostics.entries=[...diagnostics.entries.slice(-249),entry];
  try{
    // Only the most recently opened window supplies the log for the next launch.
    if(JSON.parse(localStorage.getItem(key)??'null')?.session===session)
      localStorage.setItem(key,JSON.stringify({session,entries:diagnostics.entries}));
  }catch{/* Keep the in-memory log if storage is full. */}
}

export function recordError(context:string,error:unknown){
  const message=error instanceof Error?error.stack??error.message:String(error);
  recordActivity(context,message,'error');
}
export async function invoke<T>(command:string,args?:Record<string,unknown>):Promise<T>{
  try{return await nativeInvoke<T>(command,args);}
  catch(error){recordError(command,error);throw error;}
}
window.addEventListener('error',e=>recordError('Editor',e.error??e.message));
window.addEventListener('unhandledrejection',e=>recordError('Unhandled promise',e.reason));
