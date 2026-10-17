import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { CITY_STATES, cityAtView, stateAtPoint, navigationFromURL, navigationURL } from '../src/geography-data.js';
const { features } = JSON.parse(readFileSync(new URL('../assets/geography/us-states.geojson', import.meta.url)));
const sf = { slug: 'sf', bbox: { west: -122.5247, east: -122.3366, south: 37.6983, north: 37.8312 } };

test('national geography includes every state plus DC, including Alaska across the dateline', () => {
  assert.equal(new Set(features.map(f => f.properties.code)).size, 51);
  for (const [point, code] of [
    [{ lat: 37.77, lng: -122.42 }, 'CA'],
    [{ lat: 34.05, lng: -118.24 }, 'CA'],
    [{ lat: 40.71, lng: -74.01 }, 'NY'],
    [{ lat: 61.22, lng: -149.9 }, 'AK'],
    [{ lat: 21.31, lng: -157.86 }, 'HI'],
  ]) assert.equal(stateAtPoint(features, point)?.properties.code, code);
  assert.equal(stateAtPoint(features, {lat: 35, lng: -140}), null);
  const alaska = features.find(f => f.properties.code === 'AK');
  const coordinates = alaska.geometry.coordinates.flat(Infinity);
  const longitudes = coordinates.filter((_, i) => i % 2 === 0);
  assert.ok(Math.min(...longitudes) < -180);
  assert.ok(Math.max(...longitudes) < 0); // Aleutians stay in Alaska’s map world.
});

test('zoom handoff requires both a loaded city footprint and city-level zoom', () => {
  assert.equal(cityAtView([sf], { lat: 37.77, lng: -122.42 }, 10), null);
  assert.equal(cityAtView([sf], { lat: 37.77, lng: -122.42 }, 11), sf);
  assert.equal(cityAtView([sf], { lat: 38.58, lng: -121.49 }, 12), null);
  assert.equal(cityAtView([], { lat: 37.77, lng: -122.42 }, 12), null);
  assert.equal(CITY_STATES.sf, 'CA');
});

test('geographic URLs preserve backend and workspace while clearing stale branch data', () => {
  const href = 'http://localhost:5176/index.html?backend=local&port=8083&ws=test&city=sf&branch=old&residents=10000';
  const state = navigationURL(href, { view: 'state', state: 'CA' });
  assert.equal(state.searchParams.get('ws'), 'test');
  assert.equal(state.searchParams.get('port'), '8083');
  assert.equal(state.searchParams.get('branch'), null);
  assert.deepEqual(navigationFromURL(state), { view: 'state', state: 'CA' });
  assert.deepEqual(navigationFromURL(navigationURL(state, { view: 'us' })), { view: 'us' });
  assert.deepEqual(navigationFromURL(navigationURL(state, { view: 'city', city: 'sf' })), { view: 'city', city: 'sf' });
  assert.deepEqual(navigationFromURL(href), { view: 'city', city: 'sf' });
  assert.deepEqual(navigationFromURL('http://localhost/'), { view: 'us' });
});

test('state containment respects polygon holes and disconnected islands', () => {
  const square = (x,y,size) => [[x,y],[x+size,y],[x+size,y+size],[x,y+size],[x,y]];
  const f = { geometry: { type: 'MultiPolygon', coordinates: [[square(0,0,10),square(2,2,2)],[square(20,20,1)]] } };
  assert.equal(stateAtPoint([f], {lat:1,lng:1}),f);
  assert.equal(stateAtPoint([f], {lat:3,lng:3}),null);
  assert.equal(stateAtPoint([f], {lat:20.5,lng:20.5}),f);
});

test('residents fade continuously with zoom and preload before they are visible', async () => {
  const { residentOpacity, nearbyCity } = await import('../src/geography-data.js');
  assert.equal(residentOpacity(10.9), 0);
  assert.equal(residentOpacity(11), 0);
  assert.equal(residentOpacity(11.75), .5);
  assert.equal(residentOpacity(12.5), 1);
  assert.equal(residentOpacity(18), 1);
  const center = {lat:37.77,lng:-122.42};
  assert.equal(nearbyCity([sf],center,8),null);
  assert.equal(nearbyCity([sf],center,9),sf);
  assert.equal(nearbyCity([sf],{lat:34,lng:-118},12),null);
});

test('resident world coordinates share the geographic camera in both directions', async () => {
  const { SFMap } = await import('../src/map.js');
  const { MAP } = await import('../src/config.js');
  const map = Object.create(SFMap.prototype);
  map.imgW = 2144; map.imgH = 1920;
  map.setGeographicView({
    project: (lon,lat) => ({x:lon*12+1000,y:lat*-8+700}),
    unproject: (x,y) => ({lng:(x-1000)/12,lat:(y-700)/-8}),
  });
  const world = map.lonlatToWorld(-122.42,37.77);
  const screen = map.worldToScreen(world.x,world.y);
  assert.ok(Math.abs(screen.x-(-122.42*12+1000))<1e-8);
  assert.ok(Math.abs(screen.y-(37.77*-8+700))<1e-8);
  const reverse = map.screenToWorld(screen.x,screen.y);
  assert.ok(Math.abs(reverse.x-world.x)<1e-8);
  assert.ok(Math.abs(reverse.y-world.y)<1e-8);
  assert.equal(MAP.bbox.west,-122.5247);
});
