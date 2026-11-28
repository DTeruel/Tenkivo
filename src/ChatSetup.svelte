<script lang="ts">
  import workerCode from '../chat-service/worker.mjs?raw';
  import databaseRules from '../chat-service/database.rules.json?raw';
  import {invoke,recordError} from './diagnostics.svelte';
  let {repo,endpoint,connect,back,reset}:{repo:string;endpoint:string;connect:(url:string,token:string)=>void;back:()=>void;reset:()=>void}=$props();
  let address=$state(''),token=$state(''),feedback=$state('');
  import {onMount} from 'svelte';
  onMount(()=>{address=endpoint;});
  async function visit(url:string){
    try{await invoke('open_web',{url});}catch(e){feedback=String(e);}
  }
  async function copy(value:string){
    try{await navigator.clipboard.writeText(value);feedback='Copied to clipboard.';}catch(e){recordError('Chat setup',e);feedback='Copy failed. Select and copy the text below.';}
  }
  function submit(e:SubmitEvent){
    e.preventDefault();
    try{
      const url=new URL(address.trim());
      if(url.protocol!=='https:' || url.username || url.password || url.search || url.hash)throw Error('Enter a valid HTTPS Worker URL.');
      if(!token.trim().startsWith('github_pat_'))throw Error('Use a fine-grained GitHub token.');
      connect(url.href,token.trim());
    }catch(e){feedback=String(e);}
  }
</script>
<section class="setup">
  <h3>Connect this project</h3>
  <p>Ask your project owner for the chat Worker URL. Everyone on the team uses the same URL for {repo}.</p>
  <form onsubmit={submit}>
    <label>Trusted Worker URL<input required type="url" placeholder="https://tenkivo-chat-auth.team.workers.dev" bind:value={address}/></label>
    <label>Chat access token<input type="password" required autocomplete="off" placeholder="github_pat_…" bind:value={token}/></label>
    <p>Create a fine-grained token under the repository owner. Select only <strong>{repo}</strong> and keep the default read-only Metadata permission. Your token is sent to the Worker above and held in memory for this project session.</p>
    <button type="button" onclick={()=>visit('https://github.com/settings/personal-access-tokens/new')}>Create GitHub token ↗</button>
    <details><summary>Set up a service for your team</summary>
      <ol>
        <li>Create a Firebase Spark project. Register a Web app and enable Authentication. Create a Realtime Database in locked mode. In its Rules tab, paste and publish the supplied rules.
          <div><button type="button" onclick={()=>visit('https://console.firebase.google.com/')}>Open Firebase ↗</button><button type="button" onclick={()=>copy(databaseRules)}>Copy database rules</button></div>
          <details><summary>View rules</summary><textarea readonly value={databaseRules}></textarea></details>
        </li>
        <li>In Google Cloud, select the same project. Create a dedicated service account with no project roles. Under Keys, create a JSON key. Keep it private.
          <div><button type="button" onclick={()=>visit('https://console.cloud.google.com/iam-admin/serviceaccounts')}>Open service accounts ↗</button></div>
        </li>
        <li>In Cloudflare Workers &amp; Pages, create a Worker. In Edit code, replace the starter code with the supplied Worker and deploy it.
          <div><button type="button" onclick={()=>visit('https://dash.cloudflare.com/')}>Open Cloudflare ↗</button><button type="button" onclick={()=>copy(workerCode)}>Copy Worker code</button></div>
          <details><summary>View Worker code</summary><textarea readonly value={workerCode}></textarea></details>
        </li>
        <li>Under Worker Settings → Variables and Secrets, set <code>PROJECT_ID</code> and <code>API_KEY</code> from the Firebase web app, <code>DATABASE_URL</code> from Realtime Database and <code>CLIENT_EMAIL</code> from the service account. Add <code>PRIVATE_KEY</code> as a secret with the multiline PEM key. Deploy and share the HTTPS Worker URL with your team.</li>
      </ol>
    </details>
    <div class="buttons"><button type="submit">Connect</button><button type="button" onclick={back}>Back</button>{#if endpoint}<button type="button" onclick={reset}>Forget service</button>{/if}</div>
  </form>
  {#if feedback}<small role="status">{feedback}</small>{/if}
</section>
<style>
  .setup{display:grid;gap:8px}.setup h3,.setup p{margin:0}.setup form{display:grid;gap:10px}.setup p,.setup li{line-height:1.45}.setup ol{padding-left:1.4rem}.setup li{margin:12px 0}.setup li div,.buttons{display:flex;flex-wrap:wrap;gap:6px;margin-top:8px}.setup textarea{width:100%}.setup code{overflow-wrap:anywhere}
</style>
