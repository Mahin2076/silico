import { US_BOUNDS, REGION_BOUNDS, cityCenter, navigationFromURL, navigationURL, residentOpacity, nearbyCity } from './geography-data.js';
import { createSmoothMapCamera } from './smooth-map-camera.js';

// A single live satellite camera from continent to street. The existing resident
// renderer is a transparent layer on top, never a second background to swap in.
export async function createGeography({ cities, renderer, onEnter, onLeave, canLeave, onEntered }) {
  const $ = id => document.getElementById(id);
  const canvas = $('geo-map'), residentCanvas = $('map'), ui = $('ui'), status = $('geo-status');
  residentCanvas.tabIndex = -1;
  const L = globalThis.L, initial = navigationFromURL(location.href);
  const reduced = () => matchMedia('(prefers-reduced-motion: reduce)').matches;
  let loaded = null, loading = false, cityVisible = false, failedAt = 0, settling = false;
  if (!L) { status.textContent = 'The map could not load. Please refresh to try again.'; return { setCity() {} }; }
  const map = L.map(canvas, { zoomControl: false, scrollWheelZoom: false, doubleClickZoom: false,
    minZoom: 3, maxZoom: 18, zoomSnap: 0, zoomAnimation: false, fadeAnimation: !reduced(), inertia: !reduced(), worldCopyJump: false, maxBounds: US_BOUNDS, maxBoundsViscosity: 1, bounceAtZoomLimits: false });
  let region = 'US';
  const regionBounds = () => L.latLngBounds(region === 'AK' ? REGION_BOUNDS.AK : US_BOUNDS);
  const overviewZoom = () => map.getBoundsZoom(regionBounds(), false, L.point(70, 70));
  map.fitBounds(US_BOUNDS, { padding: [35, 35], animate: false });
  map.setMinZoom(overviewZoom());
  const camera = createSmoothMapCamera(map, canvas, L, reduced);
  const imagery = L.tileLayer('https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}', {
    maxZoom: 19, keepBuffer: 4, updateWhenZooming: true, updateWhenIdle: false,
    attribution: 'Imagery © Esri, Maxar, Earthstar Geographics, GIS User Community',
  }).addTo(map);
  const labels = L.tileLayer('https://services.arcgisonline.com/ArcGIS/rest/services/Reference/World_Boundaries_and_Places/MapServer/tile/{z}/{y}/{x}', {
    maxZoom: 18, updateWhenZooming: true, updateWhenIdle: false, attribution: 'Labels © Esri',
  }).addTo(map);
  let failures = 0;
  imagery.on('tileerror', () => { if (++failures === 4) status.textContent = 'Satellite imagery is unavailable. Check your connection.'; });
  imagery.on('tileload', () => { if (failures >= 4) status.textContent = ''; failures = 0; });

  function writeView(city) {
    const url = navigationURL(location.href, city ? { view: 'city', city: city.slug } : { view: 'us' });
    if (url.href !== location.href) history.replaceState(null, '', url);
  }
  function attachRenderer() {
    renderer.setGeographicView({
      project: (lon, lat) => map.latLngToContainerPoint([lat, lon]),
      unproject: (x, y) => map.containerPointToLatLng([x, y]),
      zoom: () => map.getZoom(),
      zoomTo: (lon, lat) => camera.flyTo([lat, lon], Math.max(14.2, map.getZoom())),
      overview: () => loaded && camera.flyTo(cityCenter(loaded), cityZoom(loaded)),
    });
    renderer.start();
  }
  function cityZoom(city) {
    const b = city.bbox;
    return Math.max(12.2, map.getBoundsZoom([[b.south, b.west], [b.north, b.east]], false, L.point(40, 40)));
  }
  function showCity(visible) {
    if (cityVisible === visible) return;
    if (!visible && !canLeave()) return;
    cityVisible = visible;
    document.body.classList.toggle('geography-open', !visible);
    ui.inert = !visible;
    if (visible) { onEntered?.(); writeView(loaded); }
    else { onLeave(); writeView(null); }
  }
  async function prepare(city) {
    if (loading || !canLeave() && cityVisible) return;
    loading = true;
    try {
      await onEnter(city.slug);
      loaded = city;
      attachRenderer();
      failedAt = 0; status.textContent = '';
    } catch {
      failedAt = Date.now();
      if (map.getZoom() >= 11) status.textContent = 'Residents couldn’t load. Keep exploring; we’ll retry shortly.';
    } finally { loading = false; refresh(); }
  }
  function refresh() {
    const zoom = map.getZoom(), center = map.getCenter();
    const candidate = nearbyCity(cities, center, zoom);
    if (!settling && candidate && candidate.slug !== loaded?.slug && !loading && Date.now() - failedAt > 5000) prepare(candidate);
    const b = loaded?.bbox;
    const inView = b && map.getBounds().intersects([[b.south, b.west], [b.north, b.east]]);
    const alpha = inView ? residentOpacity(zoom) : 0;
    residentCanvas.style.opacity = String(alpha);
    residentCanvas.dataset.residentVisibility = alpha.toFixed(3);
    if (alpha > 0 && loaded) renderer.start();
    else if (canLeave()) renderer.stop();
    showCity(Boolean(loaded && alpha > .3));
    // Fade cartographic labels away as people become the detail layer.
    labels.setOpacity(1 - alpha);
    $('geo-hint').hidden = zoom > 6;
  }
  map.on('move zoom resize', refresh);
  map.on('click', event => {
    if (cityVisible) renderer.tapGeographic(event.containerPoint.x, event.containerPoint.y);
  });
  $('geo-zoom-in').addEventListener('click', () => camera.zoomBy(1));
  $('geo-zoom-out').addEventListener('click', () => camera.zoomBy(-1));
  function setRegion(next, animate = true) {
    camera.stop(); region = next;
    // Separate US regions avoid opening an unrestricted corridor across Canada
    // and the Pacific. Hawaii is intentionally outside this explorer.
    map.setMaxBounds(regionBounds());
    map.setMinZoom(overviewZoom());
    $('geo-alaska').setAttribute('aria-pressed', String(region === 'AK'));
    if (animate) camera.flyTo(regionBounds().getCenter(), overviewZoom(), 1100);
  }
  $('geo-home').addEventListener('click', () => setRegion('US'));
  $('geo-alaska').addEventListener('click', () => setRegion(region === 'AK' ? 'US' : 'AK'));
  new ResizeObserver(() => {
    map.invalidateSize({ pan: false });
    map.setMinZoom(overviewZoom());
    map.panInsideBounds(regionBounds(), { animate: false });
  }).observe(canvas);

  async function restore(nav) {
    settling = true;
    camera.stop();
    setRegion(nav.view === 'state' && nav.state === 'AK' ? 'AK' : 'US', false);
    if (nav.view === 'city') {
      const city = cities.find(c => c.slug === nav.city);
      if (city) {
        map.setView(cityCenter(city), cityZoom(city), { animate: false });
        await prepare(city);
      }
    } else if (nav.view === 'state') {
      try {
        const response = await fetch('assets/geography/us-states.geojson');
        const { features } = await response.json();
        const f = features.find(f => f.properties.code === nav.state);
        if (f && nav.state !== 'HI') map.fitBounds(L.geoJSON(f).getBounds(), { padding: [45, 45], animate: false });
      } catch { /* Keep the map usable if the optional deep-link outline fails. */ }
    } else map.fitBounds(US_BOUNDS, { padding: [35, 35], animate: false });
    settling = false; refresh();
  }
  window.addEventListener('popstate', () => restore(navigationFromURL(location.href)));
  document.body.classList.add('geography-open', 'continuous-map'); ui.inert = true;
  await restore(initial);
  return {
    setCity(slug) {
      // Existing city-switcher changes still use this camera. Loading a nearby
      // population while zooming must never recenter or interrupt the gesture.
      if (loading || !cityVisible || loaded?.slug === slug) return;
      const city = cities.find(c => c.slug === slug);
      if (city) { loaded = city; camera.flyTo(cityCenter(city), cityZoom(city)); }
    },
  };
}
