<script lang="ts">
  import {onMount,tick} from 'svelte';
  import {initializeApp,deleteApp} from 'firebase/app';
  import {initializeAuth,inMemoryPersistence,signInWithCustomToken} from 'firebase/auth';
  import {forceWebSockets,getDatabase,ref,query,limitToLast,orderByChild,onValue,onDisconnect,set,remove,serverTimestamp,goOffline,type DatabaseReference,child} from 'firebase/database';
  import {invoke,recordError} from './diagnostics.svelte';
  import type {Project,ProjectUser,ChatMessage} from './model';
  let {project,repo,user,change}:{project:Project;repo?:string;user?:ProjectUser;change:()=>void}=$props();
  let dialog:HTMLDialogElement, list:HTMLDivElement;
  let draft=$state(''),reply=$state<ChatMessage>(),error=$state(''),sending=$state(false),limit=$state(50);
  let remote=$state<ChatMessage[]>([]),members=$state<string[]>([]),opened=$state(false),retry=$state(0);
  let messages=$derived(repo?remote:(project.chat??[]).slice(-limit));
  let room=$state.raw<DatabaseReference>(), session=crypto.randomUUID();
  const emoji=['👍','❤️','🎉','👀'];
  function fail(e:unknown){error=String(e);recordError('Project chat',e);}
  export function show(){dialog.showModal();opened=true;void bottom();}
  async function bottom(){await tick();if(list)list.scrollTop=list.scrollHeight;}
  let configured=$state(false);
  onMount(()=>{if(repo)void invoke<boolean>('online_chat_configured').then(value=>configured=value).catch(fail);});
  type Grant={apiKey:string;databaseURL:string;projectId:string;room:string;token:string};
  $effect(()=>{
    const repository=repo,identity=user?.id;void retry;
    if(!repository || !identity || (!configured && !opened))return;
    let closed=false,cleanup:()=>void=()=>{},timer:ReturnType<typeof setTimeout>;
    async function connect(){
      const grant=await invoke<Grant>('online_chat_session',{repo:repository});
      if(closed)return;
      const app=initializeApp({apiKey:grant.apiKey,projectId:grant.projectId,databaseURL:grant.databaseURL},crypto.randomUUID());
      forceWebSockets();
      const db=getDatabase(app),auth=initializeAuth(app,{persistence:inMemoryPersistence});
      const root=ref(db,'rooms/'+grant.room),here=child(root,'presence/'+identity+'/'+session);
      const stops:(()=>void)[]=[];
      cleanup=()=>{stops.forEach(stop=>stop());goOffline(db);void deleteApp(app);};
      await signInWithCustomToken(auth,grant.token);
      if(closed)return;
      error='';room=root;
      stops.push(onValue(child(root,'presence'),snap=>{
        members=[...new Set(Object.values(snap.val()??{}).flatMap(v=>Object.values(v as Record<string,string>)))];
      },fail));
      stops.push(onValue(ref(db,'.info/connected'),snap=>{
        if(snap.val())void onDisconnect(here).remove().then(()=>{if(!closed)return set(here,user?.name);}).catch(fail);
      }));
      timer=setTimeout(()=>{retry++;},12*60_000);
    }
    void connect().catch(e=>{if(!closed)fail(e);});
    return ()=>{closed=true;clearTimeout(timer);room=undefined;members=[];remote=[];cleanup();};
  });
  $effect(()=>{
    if(!room)return;
    return onValue(query(child(room,'messages'),orderByChild('time'),limitToLast(limit)),snap=>{
      const follow=!list || list.scrollHeight-list.scrollTop-list.clientHeight<40 || !remote.length;
      const next:ChatMessage[]=[];snap.forEach(node=>{next.push({...node.val(),id:node.key});});
      remote=next;if(follow)void bottom();
    },fail);
  });
  async function send(){
    if(!user || sending || !draft.trim() || (repo && !room))return;
    const msg:ChatMessage={id:crypto.randomUUID(),author:user.name,user:user.id,time:Date.now(),text:draft.trim(),...(reply?{reply:reply.id}:{})};
    sending=true;
    try{
      if(room){const {id,...body}=msg;await set(child(room,'messages/'+id),{...body,time:serverTimestamp()});}
      else{project.chat=[...project.chat??[],msg];change();}
      draft='';reply=undefined;await bottom();
    }catch(e){fail(e);}finally{sending=false;}
  }
  async function react(m:ChatMessage,symbol:string){
    if(!user || (repo && !room))return;
    const selected=m.reactions?.[symbol]?.[user.id];
    try{
      if(room){const target=child(room,'messages/'+m.id+'/reactions/'+symbol+'/'+user.id);if(selected)await remove(target);else await set(target,true);}
      else{const votes=(m.reactions??={})[symbol]??={};if(selected)delete votes[user.id];else votes[user.id]=true;change();}
    }catch(e){fail(e);}
  }
</script>
<dialog bind:this={dialog} aria-label="Project chat" onclose={()=>opened=false}>
  <header><h2>Project chat</h2><button aria-label="Close chat" onclick={()=>dialog.close()}>×</button></header>
  <small>{project.name}</small>
  {#if repo}<p>{room?'Online: '+members.join(', '):error?'Chat unavailable':'Connecting…'}</p>{/if}
  {#if error}<p role="alert">{error}</p>{#if repo}<button onclick={()=>{error='';retry++;}}>Retry</button>{/if}{/if}
  <div class="messages" bind:this={list}>
    {#if messages.length===limit}<button onclick={()=>limit+=50}>Earlier messages</button>{/if}
    {#each messages as m (m.id)}
      <article><strong>{m.author}</strong> <small>{new Date(m.time).toLocaleString(undefined,{dateStyle:'short',timeStyle:'short'})}</small>
        {#if m.reply}{@const quoted=messages.find(other=>other.id===m.reply)}<blockquote>{quoted?quoted.author+' · '+quoted.text:'Reply to an earlier message'}</blockquote>{/if}
        <p>{m.text}</p>
        <div class="actions"><button disabled={!user} onclick={()=>reply=m}>Reply</button>
        {#each emoji as symbol}<button disabled={!user} aria-label={'React with '+symbol} aria-pressed={!!(user && m.reactions?.[symbol]?.[user.id])} onclick={()=>react(m,symbol)}>{symbol} {Object.keys(m.reactions?.[symbol]??{}).length||''}</button>{/each}</div>
      </article>
    {:else}<p>{repo && !room?'':'No messages yet.'}</p>{/each}
  </div>
  {#if reply}<blockquote>Reply to {reply.author} · {reply.text} <button aria-label="Cancel reply" onclick={()=>reply=undefined}>×</button></blockquote>{/if}
  {#if !user}<small>{repo?'Sign in to chat.':'Select an editing user to send messages.'}</small>{/if}
  <textarea aria-label="Message" placeholder="Write a message…" maxlength="4000" disabled={!user || sending || (!!repo && !room)} bind:value={draft} onkeydown={e=>{if(e.key==='Enter' && !e.shiftKey && !e.isComposing){e.preventDefault();void send();}}}></textarea>
  <button disabled={!user || !draft.trim() || sending || (!!repo && !room)} onclick={send}>Send</button>
</dialog>
<style>
  dialog{width:min(420px,90vw)}header,.actions{display:flex;align-items:center;gap:5px}header{justify-content:space-between}header h2{margin:0}.messages{max-height:45vh;overflow:auto;margin-block:10px}.actions button{padding:2px 6px}.actions [aria-pressed=true]{outline:1px solid var(--accent)}article{border-top:1px solid var(--line);padding:8px 0}article p,blockquote{white-space:pre-wrap;overflow-wrap:anywhere}blockquote{border-left:2px solid var(--accent);margin:5px 0;padding-left:7px}textarea{width:100%}
</style>

