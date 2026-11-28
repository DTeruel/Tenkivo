const encode = bytes => btoa(String.fromCharCode(...bytes)).replace(/=/g, '').replace(/\+/g, '-').replace(/\//g, '_');
const part = value => encode(new TextEncoder().encode(JSON.stringify(value)));

export default {
  async fetch(request, env) {
    if (request.method !== 'POST') return new Response(null, {status:405});
    const authorization = request.headers.get('Authorization');
    if (!authorization?.startsWith('Bearer ')) return new Response(null, {status:401});
    const {repo} = await request.json();
    if (!/^[\w.-]+\/[\w.-]+$/.test(repo)) return new Response(null, {status:400});
    const headers = {Authorization:authorization, 'User-Agent':'Tenkivo chat'};
    const [userResponse, repoResponse] = await Promise.all(
      ['user', 'repos/'+repo].map(path=>fetch('https://api.github.com/'+path,{headers,redirect:'error'}))
    );
    if (!userResponse.ok || !repoResponse.ok) return new Response(null,{status:403});
    const user=await userResponse.json(), repository=await repoResponse.json();
    if (!repository.private || !repository.permissions?.pull) return new Response(null,{status:403});
    const room=String(repository.id), now=Math.floor(Date.now()/1000);
    const claims={room,login:user.login,until:(now+900)*1000};
    const payload={iss:env.CLIENT_EMAIL,sub:env.CLIENT_EMAIL,aud:'https://identitytoolkit.googleapis.com/google.identity.identitytoolkit.v1.IdentityToolkit',iat:now,exp:now+300,uid:'github:'+user.id,claims};
    const unsigned=part({alg:'RS256',typ:'JWT'})+'.'+part(payload);
    const keyData=env.PRIVATE_KEY.replace(/-----[^-]+-----|\s/g,'');
    const key=await crypto.subtle.importKey('pkcs8',Uint8Array.from(atob(keyData),c=>c.charCodeAt(0)),{name:'RSASSA-PKCS1-v1_5',hash:'SHA-256'},false,['sign']);
    const signature=await crypto.subtle.sign('RSASSA-PKCS1-v1_5',key,new TextEncoder().encode(unsigned));
    return Response.json({room,token:unsigned+'.'+encode(new Uint8Array(signature)),apiKey:env.API_KEY,databaseURL:env.DATABASE_URL,projectId:env.PROJECT_ID},{headers:{'Cache-Control':'no-store'}});
  }
};

