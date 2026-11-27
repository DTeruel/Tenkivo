import {invoke} from './diagnostics.svelte';

type Scope={repo:string;branch:string;accountId:number};
type Target=Scope & {page:string|null};
export type PageEditor={page:string;login:string;user_id:number;expires:number};

const pollInterval=12_000;
const renewalInterval=30_000;

function sameScope(a:Scope|null,b:Scope|null){
  return a?.repo===b?.repo && a?.branch===b?.branch && a?.accountId===b?.accountId;
}

export class PagePresence {
  editors=$state<PageEditor[]>([]);
  private sessionId=crypto.randomUUID();
  private target:Target|null=null;
  private published:Target|null=null;
  private nextRenewal=0;
  private pending:Promise<void>|undefined;

  setTarget(target:Target|null){
    if(sameScope(this.target,target) && this.target?.page===target?.page)return;
    if(!sameScope(this.target,target))this.editors=[];
    this.target=target;
    void this.poll();
  }

  start(){
    const timer=setInterval(()=>void this.poll(),pollInterval);
    return ()=>{clearInterval(timer);this.setTarget(null);};
  }

  async leave(){
    this.setTarget(null);
    let timeout:ReturnType<typeof setTimeout>|undefined;
    const finish=async()=>{
      const pending=this.pending;
      await pending;
      if(this.pending!==pending)await this.pending;
    };
    try{
      await Promise.race([finish(),new Promise<void>(resolve=>timeout=setTimeout(resolve,2000))]);
    }finally{clearTimeout(timeout);}
  }

  private request(scope:Scope,write:boolean,page:string|null){
    return invoke<PageEditor[]>('online_page_presence',{
      repo:scope.repo,branch:scope.branch,sessionId:this.sessionId,write,page
    });
  }

  private async refresh(target:Target|null){
    const published=this.published;
    if(published && (!target?.page || !sameScope(published,target))){
      this.published=null;
      this.nextRenewal=0;
      await this.request(published,true,null).catch(()=>{});
    }
    if(!target || this.target!==target)return;
    const write=!!target.page && (this.published?.page!==target.page || Date.now()>=this.nextRenewal);
    if(write){
      this.published=target;
      this.nextRenewal=0;
    }
    const editors=await this.request(target,write,target.page);
    if(write)this.nextRenewal=Date.now()+renewalInterval;
    if(sameScope(target,this.target)){
      const unique=new Map(editors.map(editor=>[JSON.stringify([editor.page,editor.user_id]),editor]));
      this.editors=[...unique.values()];
    }
  }

  private poll(){
    if(this.pending)return this.pending;
    const now=Date.now()/1000;
    if(this.editors.some(editor=>editor.expires<=now))this.editors=this.editors.filter(editor=>editor.expires>now);
    const target=this.target;
    this.pending=this.refresh(target).catch(()=>{}).finally(()=>{
      this.pending=undefined;
      if(this.target!==target)void this.poll();
    });
    return this.pending;
  }
}
