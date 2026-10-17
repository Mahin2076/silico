import test from 'node:test';
import assert from 'node:assert/strict';
import { createSmoothMapCamera } from '../src/smooth-map-camera.js';

class Point {
  constructor(x,y) { this.x=x; this.y=y; }
  add(p) { return new Point(this.x+p.x,this.y+p.y); }
  subtract(p) { return new Point(this.x-p.x,this.y-p.y); }
  multiplyBy(k) { return new Point(this.x*k,this.y*k); }
  divideBy(k) { return this.multiplyBy(1/k); }
}
function fixture(reduced = false) {
  let zoom=6, center={lat:37,lng:-120}, frames=new Map(), id=0, samples=[], events=[];
  const raf=globalThis.requestAnimationFrame, cancel=globalThis.cancelAnimationFrame;
  globalThis.requestAnimationFrame = cb => {frames.set(++id, cb);return id;};
  globalThis.cancelAnimationFrame = n => frames.delete(n);
  const map = {
    getMinZoom:()=>3,getMaxZoom:()=>18,getZoom:()=>zoom,getCenter:()=>center,
    getSize:()=>new Point(1000,700),on(){},options:{},
    _stop(){},_moveStart(){events.push('start');},_moveEnd(){events.push('end');},
    fire(name){events.push(name);},_limitCenter(p){return p;},
    project:(p,z)=>new Point(p.lng*2**z,p.lat*2**z),
    unproject:(p,z)=>({lng:p.x/2**z,lat:p.y/2**z}),
    containerPointToLatLng(p) { return this.unproject(this.project(center,zoom).add(p.subtract(this.getSize().divideBy(2))),zoom); },
    setView() { throw new Error('setView invalidates loaded tiles'); },
    _move(p,z,data) { center=p;zoom=z;samples.push({p,z,data}); },
  };
  const camera=createSmoothMapCamera(map,{addEventListener(){}},{latLng:p=>Array.isArray(p)?{lat:p[0],lng:p[1]}:p},()=>reduced);
  return { map,camera,samples,events,flush() {for(let t=0;t<5000&&frames.size;t+=16){const c=[...frames.values()];frames.clear();c.forEach(f=>f(t));}},restore(){globalThis.requestAnimationFrame=raf;globalThis.cancelAnimationFrame=cancel;} };
}

test('wheel zoom advances in fractional frames and keeps the cursor on the same geography', () => {
  const f=fixture();
  try {
    const pixel=new Point(320,240), before=f.map.containerPointToLatLng(pixel);
    f.camera.zoomBy(1.5,pixel);f.flush();
    assert.ok(f.samples.length>10);
    assert.equal(f.map.getZoom(),7.5);
    for(let i=1;i<f.samples.length;i++) assert.ok(f.samples[i].z>=f.samples[i-1].z);
    const after=f.map.containerPointToLatLng(pixel);
    assert.ok(Math.abs(before.lat-after.lat)<1e-10);
    assert.ok(Math.abs(before.lng-after.lng)<1e-10);
  } finally {f.restore();}
});

test('a new wheel gesture interrupts a scripted flight, and reduced motion settles immediately', () => {
  const f=fixture();
  try { f.camera.flyTo([37.7,-122.4],13);f.camera.zoomBy(-1);f.flush();assert.equal(f.map.getZoom(),5); }
  finally {f.restore();}
  const r=fixture(true);
  try {r.camera.zoomBy(2);assert.equal(r.samples.filter(s=>s.data?.flyTo).length,1);assert.deepEqual(r.events,['start','viewreset','end']);assert.equal(r.map.getZoom(),8);}
  finally {r.restore();}
});


test('scroll keeps tile levels through the gesture and ends one balanced movement', () => {
  const f=fixture();
  try {
    f.camera.zoomBy(1);f.camera.zoomBy(.5);f.flush();
    assert.deepEqual(f.events,['start','viewreset','end']);
    assert.ok(f.samples.slice(0,-1).every(s=>s.data.flyTo));
    f.camera.stop();assert.deepEqual(f.events,['start','viewreset','end']);
  } finally {f.restore();}
});

test('zoom limits and geographic bounds constrain every animation frame', () => {
  const f=fixture();
  try {
    const bounds={west:-126,east:-66,south:24,north:50};
    f.map.options.maxBounds=bounds;
    f.map._limitCenter=(p,z,b)=>{
      assert.equal(b,bounds);
      return {lat:Math.max(b.south,Math.min(b.north,p.lat)),lng:Math.max(b.west,Math.min(b.east,p.lng))};
    };
    f.camera.flyTo([0,100],-20);f.flush();
    assert.equal(f.map.getZoom(),3);
    assert.ok(f.samples.every(s=>s.p.lat>=24&&s.p.lat<=50&&s.p.lng>=-126&&s.p.lng<=-66));
    f.camera.zoomBy(100);f.flush();assert.equal(f.map.getZoom(),18);
  } finally {f.restore();}
});
