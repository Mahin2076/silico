// Load the complete population with bounded parallel requests, preserving server order.
export async function loadResidents(fetchPage, onProgress, cap = 50000) {
  const limit = 1000;
  const first = await fetchPage(0, limit);
  const total = first.total_matched;
  if (!Number.isInteger(total) || total < 0 || total > cap) throw new Error('Resident population exceeds the supported size or is invalid.');
  function validate(page, offset) {
    if (page.total_matched !== total || !Array.isArray(page.agents) || page.agents.length !== Math.min(limit, total - offset)) {
      throw new Error('The resident population changed or a page was incomplete. Please retry.');
    }
    return page.agents;
  }
  const out = validate(first, 0).slice();
  let loaded = out.length;
  onProgress?.(loaded, total);
  for (let start = limit; start < total; start += 4 * limit) {
    const offsets = Array.from({length: Math.min(4, Math.ceil((total - start) / limit))}, (_, i) => start + i * limit);
    const pages = await Promise.all(offsets.map(async offset => {
      const batch = validate(await fetchPage(offset, limit), offset);
      loaded += batch.length;
      onProgress?.(loaded, total);
      return batch;
    }));
    for (const batch of pages) out.push(...batch);
  }
  return out;
}
