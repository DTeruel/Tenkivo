<script lang="ts">
  import {onMount} from 'svelte';
  import {invoke} from './diagnostics.svelte';

  type User={id:number;login:string};
  type VerifiedUser=User & {name:string|null};
  type Member=User & {role:string;removable:boolean};
  type Invitation={id:number;invitee:User|null;permissions:string};
  type Access={collaborators:Member[];invitations:Invitation[]};
  type Removal={kind:'member';member:Member}|{kind:'invitation';invitation:Invitation};
  let {repo,onback}: {repo:string;onback:()=>void}=$props();
  let access=$state<Access>(),verified=$state<VerifiedUser>(),removal=$state<Removal>();
  let login=$state(''),working=$state(false),error=$state(''),message=$state('');

  async function run(action:()=>Promise<void>){
    if(working)return;
    working=true;error='';message='';
    try{await action();}catch(reason){error=String(reason);}
    finally{working=false;}
  }

  async function fetchAccess(){access=await invoke<Access>('online_project_access',{repo});}
  async function verify(e:SubmitEvent){
    e.preventDefault();
    await run(async()=>{verified=await invoke<VerifiedUser>('online_find_user',{login:login.trim()});});
  }
  async function invite(){
    const user=verified;
    if(!user)return;
    await run(async()=>{
      const pending=await invoke<boolean>('online_invite_editor',{repo,login:user.login,userId:user.id});
      verified=undefined;login='';
      message=pending?'Invitation sent to @'+user.login+'.':'Write access granted to @'+user.login+'.';
      await fetchAccess();
    });
  }
  async function remove(){
    const target=removal;
    if(!target)return;
    await run(async()=>{
      if(target.kind==='member'){
        await invoke('online_remove_editor',{repo,login:target.member.login,userId:target.member.id});
        message='Direct access removed for @'+target.member.login+'.';
      }else{
        await invoke('online_cancel_invitation',{repo,invitationId:target.invitation.id});
        message='Invitation cancelled.';
      }
      removal=undefined;
      await fetchAccess();
    });
  }
  onMount(()=>{void run(fetchAccess);});
</script>

<p>Manage direct access to <strong>{repo}</strong>. Invited users must accept on GitHub before the project appears in Tenkivo.</p>
{#if working}<p role="status">Contacting GitHub…</p>{/if}
{#if error}<p role="alert">{error}</p>{/if}
{#if message}<p role="status">{message}</p>{/if}
{#if access}
  <div class="log-list">
    <h3>Collaborators</h3>
    {#each access.collaborators as member (member.id)}
      <div class="row"><span><strong>@{member.login}</strong> · {member.role}</span>
        {#if member.removable}<button disabled={working} onclick={()=>{removal={kind:'member',member};}}>Remove…</button>{/if}
      </div>
    {:else}<p>No direct collaborators listed.</p>{/each}
    {#if access.invitations.length}
      <h3>Pending invitations</h3>
      {#each access.invitations as invitation (invitation.id)}
        <div class="row"><span>{invitation.invitee?'@'+invitation.invitee.login:'Email invitation'} · {invitation.permissions}</span>
          <button disabled={working} onclick={()=>{removal={kind:'invitation',invitation};}}>Cancel invitation…</button>
        </div>
      {/each}
    {/if}
  </div>
  {#if removal}
    <p>{removal.kind==='member'?'Remove direct access for @'+removal.member.login+'? Access through organization teams or roles may remain.':'Cancel this GitHub invitation?'}</p>
    <button disabled={working} onclick={()=>removal=undefined}>Back</button>
    <button disabled={working} onclick={remove}>{removal.kind==='member'?'Remove direct access':'Cancel invitation'}</button>
  {:else}
    <form onsubmit={verify}>
      <label>GitHub username<input required placeholder="@username" autocomplete="off" spellcheck="false"
        bind:value={login} oninput={()=>verified=undefined} disabled={working}/></label>
      <button type="submit" disabled={working || !login.trim()}>Verify account</button>
    </form>
    {#if verified}
      <p>Verified account <strong>@{verified.login}</strong>{verified.name?' · '+verified.name:''}.</p>
      <button disabled={working} onclick={invite}>Invite as editor</button>
    {/if}
  {/if}
{/if}
<button disabled={working} onclick={()=>run(fetchAccess)}>Refresh</button>
<button disabled={working} onclick={onback}>Project settings</button>
