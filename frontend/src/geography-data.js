// Geographic navigation metadata; population availability still comes from /cities.
export const CITY_STATES = { sf: 'CA', synth_la: 'CA', neu_york: 'NY', cybercago: 'IL', simami: 'FL' };
export const US_BOUNDS = [[24, -126], [50, -66]];
export const REGION_BOUNDS = { AK: [[51, -180], [72, -129]], HI: [[18.5, -161], [22.5, -154]] };
export function cityCenter(city) {
  const b = city.bbox;
  return [(b.south + b.north) / 2, (b.west + b.east) / 2];
}
export function cityAtView(cities, { lat, lng }, zoom) {
  if (zoom < 11) return null;
  return cities.find(({ bbox: b }) => b && lat >= b.south && lat <= b.north && lng >= b.west && lng <= b.east) || null;
}
function inRing([x, y], ring) {
  let inside = false;
  for (let i = 0, j = ring.length - 1; i < ring.length; j = i++) {
    const [xi, yi] = ring[i], [xj, yj] = ring[j];
    if ((yi > y) !== (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi) inside = !inside;
  }
  return inside;
}
export function stateAtPoint(features, { lat, lng }) {
  return features.find(({ geometry: g }) => {
    const polygons = g.type === 'Polygon' ? [g.coordinates] : g.coordinates;
    return polygons.some(([outer, ...holes]) => inRing([lng, lat], outer) && !holes.some(h => inRing([lng, lat], h)));
  }) || null;
}
export function navigationFromURL(href) {
  const p = new URL(href).searchParams;
  const view = p.get('view');
  if (view === 'us') return { view: 'us' };
  if (view === 'state' && p.get('state')) return { view: 'state', state: p.get('state') };
  if (p.get('city')) return { view: 'city', city: p.get('city') };
  if (p.get('demo') === '1' || p.get('abdemo') === '1') return { view: 'city', city: 'sf' };
  return { view: 'us' };
}
export function navigationURL(href, { view, state, city }) {
  const url = new URL(href);
  url.searchParams.set('view', view);
  url.searchParams.delete('state');
  url.searchParams.delete('branch');
  url.searchParams.delete('residents');
  if (state) url.searchParams.set('state', state);
  if (city) url.searchParams.set('city', city);
  return url;
}

export function residentOpacity(zoom) {
  const t = Math.max(0, Math.min(1, (zoom - 11) / 1.5));
  return t * t * (3 - 2 * t);
}
// Begin loading before residents become visible, without moving the camera.
export function nearbyCity(cities, { lat, lng }, zoom) {
  if (zoom < 9) return null;
  return cities.filter(c => c.bbox).map(city => {
    const [y, x] = cityCenter(city);
    return { city, distance: Math.hypot(lat - y, (lng - x) * Math.cos(lat * Math.PI / 180)) };
  }).sort((a, b) => a.distance - b.distance).find(c => c.distance < .65)?.city || null;
}
