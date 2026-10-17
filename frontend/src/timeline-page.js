import * as api from "./api.js?v=ws-1";
import { WORKSPACE } from "./workspace.js?v=2";
import { initFeedPanel } from "./feedpanel.js?v=20";

const query = new URLSearchParams(location.search);
let city = { slug: "sf", display: "San Francisco" };
let news = [];
let branch = query.get("branch") || null;
const back = new URL("index.html", location.href);
back.search = location.search;
back.searchParams.delete("branch");
back.searchParams.delete("residents");
back.searchParams.set("ws", WORKSPACE);
const backLink = document.getElementById("back-to-map");
backLink.href = back.href;
const message = document.getElementById("timeline-message");

try {
  const catalog = await api.getCities();
  const cities = catalog?.cities || [];
  city = cities.find((c) => c.slug === query.get("city")) || cities.find((c) => c.default) || cities[0] || city;
  if (city.slug !== query.get("city")) branch = null;
} catch {
  // Keep the requested supported city when the catalog is temporarily offline.
  const names = { sf: "San Francisco", neu_york: "New York City", synth_la: "Los Angeles", cybercago: "Chicago", simami: "Miami" };
  if (names[query.get("city")]) city = { slug: query.get("city"), display: names[query.get("city")] };
}
back.searchParams.set("city", city.slug);
backLink.href = back.href;
document.title = `${city.display} timeline · Silico`;
try {
  news = (await api.getNews(city.slug)).articles || [];
} catch {
  message.hidden = false;
  message.textContent = "City news couldn’t load. Refresh to try again.";
}
initFeedPanel({
  standalone: true,
  getCity: () => city.slug,
  getCityDisplay: () => city.display,
  getBranch: () => branch,
  getResidents: () => Math.max(0, Number(query.get("residents")) || 0),
  getNews: () => news,
});
