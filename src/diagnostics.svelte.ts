import {invoke as nativeInvoke} from '@tauri-apps/api/core';

type Level='info'|'warning'|'error';
type Entry={time:string;level:Level;context:string;message:string};
const key='studio-editor-errors';
function previous():Entry[]{
  try{
    const data:unknown=JSON.parse(localStorage.getItem(key)??'[]');
    return Array.isArray(data)?data.filter(e=>e && typeof e.time==='string' && typeof e.context==='string' && typeof e.message==='string')
      .slice(-250).map(e=>({...e,level:e.level==='info' || e.level==='warning'?e.level:'error'})):[];
  }catch{return [];}
}
export const diagnostics=$state({entries:previous()});
export function recordActivity(context:string,message:string,level:Level='info'){
  diagnostics.entries=[...diagnostics.entries.slice(-249),{time:new Date().toISOString(),level,context,message:message.slice(0,8000)}];
  try{localStorage.setItem(key,JSON.stringify(diagnostics.entries));}catch{/* Keep the in-memory log if storage is full. */}
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
