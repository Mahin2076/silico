// Fractional zoom on every animation frame. Native Leaflet wheel zoom advances
// in steps and otherwise leaves the resident overlay behind a CSS tile animation.
export function createSmoothMapCamera(map, element, L, reduced = () => false) {
  let frame = 0, target = null, anchor = null, anchorPixel = null, last = 0, flying = false, moving = false;
  const clamp = z => Math.max(map.getMinZoom(), Math.min(map.getMaxZoom(), z));
  // Leaflet 1.9.4's own flyTo uses this lifecycle. setView on every frame
  // emits viewprereset and destroys all loaded tiles, exposing the blue canvas.
  // Keep parent tiles transformed under new imagery until replacements arrive.
  function begin() {
    if (moving) return;
    map._stop(); moving = true; map._moveStart(true);
  }
  function move(center, zoom, final = false) {
    zoom = clamp(zoom);
    center = map._limitCenter(center, zoom, map.options.maxBounds);
    map._move(center, zoom, final ? undefined : { flyTo: true });
  }
  function finish() {
    if (!moving) return;
    move(map.getCenter(), map.getZoom(), true);
    // A final same-zoom _move does not emit zoom; reset the grid without
    // invalidating its loaded tiles so pruning resumes after the gesture.
    map.fire('viewreset');
    moving = false; map._moveEnd(true);
  }
  function stop() {
    cancelAnimationFrame(frame); frame = 0; target = null; flying = false;
    map._stop(); finish();
  }
  function step(now) {
    target = clamp(target);
    const dt = Math.min(40, last ? now - last : 16); last = now;
    const z = map.getZoom();
    const next = Math.abs(target - z) < .002 ? target : z + (target - z) * (1 - Math.exp(-dt / 70));
    const center = map.project(anchor, next).subtract(anchorPixel.subtract(map.getSize().divideBy(2)));
    move(map.unproject(center, next), next);
    if (next !== target) frame = requestAnimationFrame(step);
    else { frame = 0; target = null; finish(); }
  }
  function zoomBy(delta, pixel = map.getSize().divideBy(2)) {
    if (flying) stop();
    anchor = map.containerPointToLatLng(pixel); anchorPixel = pixel;
    target = clamp((target ?? map.getZoom()) + delta);
    begin();
    if (reduced()) {
      const center = map.project(anchor, target).subtract(pixel.subtract(map.getSize().divideBy(2)));
      move(map.unproject(center, target), target); target = null; finish(); return;
    }
    if (!frame) { last = 0; frame = requestAnimationFrame(step); }
  }
  element.addEventListener('wheel', event => {
    event.preventDefault();
    const factor = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? element.clientHeight : 1;
    const delta = -event.deltaY * factor * (event.ctrlKey ? .008 : .0035);
    zoomBy(Math.max(-1.5, Math.min(1.5, delta)), map.mouseEventToContainerPoint(event));
  }, { passive: false });
  element.addEventListener('pointerdown', stop);
  element.addEventListener('keydown', event => {
    if (event.key === '+' || event.key === '=' || event.key === '-') {
      event.preventDefault(); event.stopPropagation(); zoomBy(event.key === '-' ? -1 : 1);
    }
  }, true);
  map.on('dblclick', event => zoomBy(1.4, event.containerPoint));
  function flyTo(center, zoom, duration = 1100) {
    stop();
    begin();
    if (reduced()) { move(L.latLng(center), zoom); finish(); return; }
    flying = true;
    const fromZoom = map.getZoom(), from = map.project(map.getCenter(), 0), to = map.project(L.latLng(center), 0);
    let start;
    function flight(now) {
      start ??= now;
      const t = Math.min(1, (now - start) / duration), ease = t * t * (3 - 2 * t);
      const p = from.add(to.subtract(from).multiplyBy(ease));
      move(map.unproject(p, 0), fromZoom + (clamp(zoom) - fromZoom) * ease);
      frame = t < 1 ? requestAnimationFrame(flight) : 0;
      if (!frame) { flying = false; finish(); }
    }
    frame = requestAnimationFrame(flight);
  }
  return { zoomBy, flyTo, stop };
}
