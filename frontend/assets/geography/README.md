# State outlines

`us-states.geojson` derives from the US Census Bureau's 2024 cartographic
state boundaries at 1:20 million scale:
https://www2.census.gov/geo/tiger/GENZ2024/shp/cb_2024_us_state_20m.zip

Contains the 50 states and District of Columbia. Properties are reduced to
`name` and postal `code`; coordinates rounded to five decimals. Alaska's
positive longitudes are shifted by −360 degrees so the Aleutian Islands
render beside mainland Alaska. These generalized outlines are for navigation,
not resident placement or demographic aggregation. Census geography is public
domain. Existing population data and city assets are unchanged.

The geographic viewer uses locally vendored Leaflet 1.9.4 (BSD-2-Clause,
see `../../vendor/leaflet/LICENSE`) and the same live Esri World Imagery
service already used by the resident map, with Esri place labels. These
public tile endpoints do not require a new API key. Provider attribution is
visible on the map. Network access is needed for satellite imagery.
