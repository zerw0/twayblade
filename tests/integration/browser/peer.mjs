// Real pinned Element Call SPA, isolated Chromium profile; no widget bridge.
// Stdout is a private counts-only JSON protocol. Never forward browser console,
// page errors, URLs, HTTP bodies, keys or credentials to fixture logs.
import {chromium} from 'playwright';
import {writeFileSync} from 'node:fs';
import {createInterface} from 'node:readline';
import {spawnSync} from 'node:child_process';
import {resolve} from 'node:path';

const lines=createInterface({input:process.stdin});
const config=await new Promise(r=>lines.once('line',line=>r(JSON.parse(line))));
let browser;
try{
  const trust=JSON.parse(spawnSync('python3',['tests/integration/call-fixture-trust.py'],{encoding:'utf8'}).stdout);
  const root=resolve('test-data/local/integration');
  // 440 Hz PCM fixture file. Chromium uses it as an explicit fake microphone.
  const samples=48000*2;const wav=Buffer.alloc(44+samples*2);
  wav.write('RIFF');wav.writeUInt32LE(wav.length-8,4);wav.write('WAVEfmt ',8);wav.writeUInt32LE(16,16);wav.writeUInt16LE(1,20);wav.writeUInt16LE(1,22);wav.writeUInt32LE(48000,24);wav.writeUInt32LE(96000,28);wav.writeUInt16LE(2,32);wav.writeUInt16LE(16,34);wav.write('data',36);wav.writeUInt32LE(samples*2,40);
  for(let i=0;i<samples;i++)wav.writeInt16LE(Math.round(8000*Math.sin(i*440*2*Math.PI/48000)),44+i*2);
  writeFileSync(root+'/tone.wav',wav);
  const pattern=Buffer.concat([Buffer.from('YUV4MPEG2 W640 H360 F10:1 Ip A1:1 C420jpeg\n'),...Array.from({length:20},()=>{
    const frame=Buffer.alloc(640*360*3/2,128);for(let y=0;y<360;y++)for(let x=0;x<640;x++)frame[y*640+x]=[30,128,220][Math.min(2,Math.floor(x*3/640))];return Buffer.concat([Buffer.from('FRAME\n'),frame]);
  })]);
  writeFileSync(root+'/pattern.y4m',pattern);
  browser=await chromium.launch({headless:true,args:[`--ignore-certificate-errors-spki-list=${trust.spki}`,'--use-fake-ui-for-media-stream','--use-fake-device-for-media-stream',`--use-file-for-fake-video-capture=${root}/pattern.y4m`,`--use-file-for-fake-audio-capture=${root}/tone.wav`,'--autoplay-policy=no-user-gesture-required']});
  const context=await browser.newContext({permissions:['microphone','camera']});
  // Restrict the headless peer to fixture loopback services and bundled assets.
  await context.route('**/*',route=>{const url=new URL(route.request().url());return ['https:','http:'].includes(url.protocol)&&url.hostname!=='127.0.0.1'?route.abort():route.continue();});
  const page=await context.newPage();
  const errors={},httpFailures={};
  page.on('console',message=>{if(['error','warning'].includes(message.type())){const text=message.text();for(const tag of ['OpenID','SFU','JWT','WebSocket','MatrixRTC','LiveKit','permission','E2EE','encryption','Unsupported','Error','failed','focus','Connection','codec','worker'])if(text.toLowerCase().includes(tag.toLowerCase()))errors[tag]=(errors[tag]??0)+1;}});
  page.on('websocket',socket=>{errors.socketCreated=(errors.socketCreated??0)+1;socket.on('socketerror',message=>{for(const tag of ['ERR_CERT','403','404','401','500','handshake','ERR_CONNECTION'])if(message.includes(tag))errors['socket:'+tag]=(errors['socket:'+tag]??0)+1;});});
  page.on('pageerror',error=>{errors.pageError=(errors.pageError??0)+1;const stack=error.stack??'';const position=stack.match(/assets\/([A-Za-z0-9_.-]+\.js):(\d+):(\d+)/);if(position)errors.pageErrorPosition={file:position[1],line:Number(position[2]),column:Number(position[3])};const message=error.message;for(const tag of ['is not a function','Cannot read properties','Worker','WebSocket','permission','certificate','undefined','not supported','denied','SharedArrayBuffer'])if(message.includes(tag))errors['page:'+tag]=true;});
  page.on('response',response=>{if(response.status()>=400){const path=new URL(response.url()).pathname;for(const tag of ['/sfu/get','/get_token','/config.json','/rtc','/openid','/keys'])if(path.includes(tag)){const key=(path.startsWith('/_matrix')?'matrix:':'sfu:')+tag+':'+response.status();httpFailures[key]=(httpFailures[key]??0)+1;}}});

  await page.goto('https://127.0.0.1:22449/');
  const session=await page.evaluate(async data=>{
    const response=await fetch('https://127.0.0.1:18448/_matrix/client/v3/login',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({type:'m.login.password',identifier:{type:'m.id.user',user:data.username},password:data.password,initial_device_display_name:'Isolated Element Call peer'})});
    if(!response.ok)throw new Error('login');
    const session=await response.json();localStorage.setItem('matrix-auth-store',JSON.stringify({...session,passwordlessUser:false}));return {device_id:session.device_id,user_id:session.user_id};
  },config);
  await page.addInitScript(()=>{
    window.fixtureEvidence={audio:0,video:0};window.fixturePCs=[];
    const Original=window.RTCPeerConnection;
    window.RTCPeerConnection=new Proxy(Original,{construct(Target,args){const pc=new Target(...args);window.fixturePCs.push(pc);if(window.fixturePCs.length>32)window.fixturePCs.shift();pc.addEventListener('track',event=>{
      if(event.track.kind==='audio'){
        const audio=new AudioContext();
        const input=audio.createMediaStreamSource(new MediaStream([event.track]));
        const module=new Blob([`class FixtureMeter extends AudioWorkletProcessor{constructor(){super();this.blocks=0;this.signal=0;}process(inputs){const channel=inputs[0]?.[0];if(channel){let sum=0;for(const sample of channel)sum+=sample*sample;if(Math.sqrt(sum/channel.length)>.01)this.signal++;if(++this.blocks>=20){this.port.postMessage(this.signal);this.blocks=0;this.signal=0;}}return true;}}registerProcessor('fixture-meter',FixtureMeter);`],{type:'text/javascript'});
        const moduleUrl=URL.createObjectURL(module);
        audio.audioWorklet.addModule(moduleUrl).then(()=>{URL.revokeObjectURL(moduleUrl);const meter=new AudioWorkletNode(audio,'fixture-meter');meter.port.onmessage=message=>{window.fixtureEvidence.audio+=message.data;};input.connect(meter);meter.connect(audio.destination);});
      }else{
        const video=document.createElement('video');video.muted=true;video.autoplay=true;video.srcObject=new MediaStream([event.track]);video.play().catch(()=>{});
        const canvas=document.createElement('canvas');canvas.width=64;canvas.height=48;const ctx=canvas.getContext('2d',{willReadFrequently:true});
        const decoded=()=>{ctx.drawImage(video,0,0,64,48);const data=ctx.getImageData(0,0,64,48).data;const left=(24*64+10)*4,right=(24*64+53)*4;if(data[right]-data[left]>100)window.fixtureEvidence.video++;video.requestVideoFrameCallback(decoded);};video.requestVideoFrameCallback(decoded);
      }
    });return pc;}});
  });
  for(const key of Object.keys(errors))delete errors[key];for(const key of Object.keys(httpFailures))delete httpFailures[key];
  await page.goto('https://127.0.0.1:22449/room/#?'+new URLSearchParams({roomId:config.room_id,perParticipantE2EE:'true'}));
  await page.waitForFunction(()=>window.matrixclient?.getRooms().length>0,{},{timeout:30000});
  await page.evaluate(async room=>{await window.matrixclient.joinRoom(room);},config.room_id);
  const join=page.getByTestId('lobby_joinCall');await join.waitFor({timeout:30000});
  const stopVideo=page.getByRole('button',{name:'Stop video',exact:true});if(await stopVideo.isVisible())await stopVideo.click();
  await join.click();
  await page.getByTestId('incall_mute').waitFor({timeout:30000});
  const mic=page.getByTestId('incall_mute');if((await mic.getAttribute('aria-label')??'').toLowerCase().includes('unmute'))await mic.click();
  await page.waitForFunction(()=>window.rtcSession?.isJoined(),{},{timeout:30000});
  process.stdout.write(JSON.stringify({ready:true,...session})+'\n');
  let finished=false,enableVideo=false;lines.on('line',line=>{const command=JSON.parse(line);if(command.stop)finished=true;if(command.video)enableVideo=true;});
  while(!finished){
    if(enableVideo){enableVideo=false;const video=page.getByTestId('incall_videomute');if((await video.getAttribute('aria-label')??'').toLowerCase().includes('start'))await video.click();}
    const evidence=await page.evaluate(room=>{
      const client=window.matrixclient;const session=window.rtcSession??client?.matrixRTC?.getRoomSession(client.getRoom(room));
      return {joined:session?.isJoined()??false,connections:window.fixturePCs.map(pc=>pc.connectionState),participants:session?.memberships?.length??0,keys:session?.statistics?.counters?.roomEventEncryptionKeysReceived??0,audio:window.fixtureEvidence.audio,video:window.fixtureEvidence.video};
    },config.room_id);
    process.stdout.write(JSON.stringify({...evidence,errors,httpFailures})+'\n');await new Promise(r=>setTimeout(r,500));
  }
  await browser.close();
}catch{
  process.stdout.write(JSON.stringify({failed:true})+'\n');if(browser)await browser.close();process.exitCode=1;
}
lines.close();
