// One-off: arrows on Orca scene, Orca Deck clock/summary on the side panel, Grabación scene. Run with Stream Dock stopped.
import { readFileSync, writeFileSync, copyFileSync, mkdirSync } from 'node:fs';
import { randomUUID } from 'node:crypto';
const P = process.env.APPDATA + '/HotSpot/StreamDock/profiles/';
const ORCA = 'R0KZ54Z0-063U-947X-31BI-GG95R8F4HT88', REC = 'H33G4VTZ-25Z8-PYN7-R34M-ST39SX8TRT2Q';
const here = new URL('.', import.meta.url).pathname.replace(/^\/(\w:)/, '$1');
const load = (id) => JSON.parse(readFileSync(`${P}${id}.sdProfile/manifest.json`, 'utf8'));
const save = (id, j) => {
  copyFileSync(`${P}${id}.sdProfile/manifest.json`, `${P}${id}.sdProfile/manifest.json.pre-orcadeck.bak`);
  writeFileSync(`${P}${id}.sdProfile/manifest.json`, JSON.stringify(j, null, 4));
};
const img = (id, name) => { mkdirSync(`${P}${id}.sdProfile/Images`, { recursive: true }); copyFileSync(`${here}${name}`, `${P}${id}.sdProfile/Images/${name}`); return name; };
const act = (UUID, Name, Settings = {}, Image = '') => ({ ActionID: randomUUID(), Controller: '', Name, Settings, State: 0, States: [{ Image }], UUID });
const scene = (target, arrow) => act('com.hotspot.streamdock.profile.rotate', 'Scene Shift', { DeviceUUID: '', ProfileUUID: target }, arrow);

// 1+2. Orca: plain arrows; side panel = clock + agent summary.
const o = load(ORCA);
const L = img(ORCA, 'ARROW_LEFT.png'), R = img(ORCA, 'ARROW_RIGHT.png');
o.Actions['2,0'].States = [{ Image: R }]; // -> Grabación
o.Actions['4,2'].States = [{ Image: L }]; // -> Monitorización
o.Actions['5,0'] = act('com.miguelpicado.orcadock.clock', 'Clock', {}, 'imgs/action');
o.Actions['5,1'] = act('com.miguelpicado.orcadock.summary', 'Agent summary', {}, 'imgs/action');
save(ORCA, o);

// 3. Grabación ("col,row"): Zoom on top, Insta360 modes in the middle, back + zoom/gimbal + record at the bottom.
const r = load(REC);
const z = (a, n) => act(`com.lostdomain.zoom.${a}`, n);
const set = (n, option, secondOption) => act('com.insta360.webcam.setter', n, secondOption ? { option, secondOption } : { option }, 'imgs/actions/setter/default/connect');
const adj = (n, zoomValue) => act('com.insta360.webcam.adjust', n, { option: 'adjustZoom', zoomValue }, 'imgs/actions/adjust/default/on');
r.Actions = {
  '0,0': z('mutetoggle', 'Mute Toggle'), '1,0': z('videotoggle', 'Video Toggle'), '2,0': z('sharetoggle', 'Share Toggle'),
  '3,0': z('focus', 'Focus'), '4,0': z('leave', 'Leave Meeting'),
  '0,1': set('AI tracking', 'AITracking', 'autoComposition'), '1,1': set('AI zoom', 'aiZoom', 'halfBody'),
  '2,1': set('Desk view', 'deskView'), '3,1': set('Whiteboard', 'whiteboard'), '4,1': set('Privacy', 'private'),
  '0,2': scene(ORCA, img(REC, 'ARROW_LEFT.png')),
  '1,2': adj('Zoom -', '-0.5'), '2,2': adj('Zoom +', '0.5'),
  '3,2': act('com.insta360.webcam.reset', 'Reset gimbal', { option: 'resetPosition' }, 'imgs/actions/reset/default'),
  '4,2': z('recordlocaltoggle', 'Local Record Toggle'),
};
save(REC, r);
console.log('profiles written');
