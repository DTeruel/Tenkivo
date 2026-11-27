import {invoke} from './diagnostics.svelte';
import type {Attachment} from './model';

export async function materializeAttachment(attachment:Attachment, repo?:string):Promise<Attachment>{
  if(!attachment.shared)return attachment;
  if(!repo)throw Error('Open the online project to download this attachment.');
  const path=await invoke<string>('online_attachment_path',{repo,attachment});
  return {path,local:false,folder:false};
}
