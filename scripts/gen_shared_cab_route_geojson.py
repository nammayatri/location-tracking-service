#!/usr/bin/env python3
"""Generate route_geo_json_config/<routeCode>.geojson for shared-cab routes from a GTFS feed.

The feed has no shapes.txt, so the LineString is the ordered stop points of the route's trip
(stop_times.txt by stop_sequence). Stop features follow route_geo_json_config/EB12-U.geojson.
Distance/duration between stops are not in the file: LTS computes them when it loads the route.

    python3 scripts/gen_shared_cab_route_geojson.py <gtfs dir or .zip> [out_dir]
"""
import csv
import io
import json
import sys
import zipfile
from pathlib import Path


def read_table(src, name):
    if Path(src).is_dir():
        text = (Path(src) / name).read_text(encoding="utf-8-sig")
    else:
        with zipfile.ZipFile(src) as z:
            member = next(n for n in z.namelist() if n.endswith("/" + name) or n == name)
            text = z.read(member).decode("utf-8-sig")
    return list(csv.DictReader(io.StringIO(text)))


def main():
    src = sys.argv[1]
    out_dir = Path(sys.argv[2]) if len(sys.argv) > 2 else Path(__file__).resolve().parent.parent / "route_geo_json_config"
    out_dir.mkdir(parents=True, exist_ok=True)

    stops = {s["stop_id"]: s for s in read_table(src, "stops.txt")}
    trips_by_route = {}
    for t in read_table(src, "trips.txt"):
        trips_by_route.setdefault(t["route_id"], t["trip_id"])
    times = {}
    for st in read_table(src, "stop_times.txt"):
        times.setdefault(st["trip_id"], []).append(st)

    for route_id, trip_id in sorted(trips_by_route.items()):
        seq = sorted(times[trip_id], key=lambda r: int(r["stop_sequence"]))
        points = [stops[r["stop_id"]] for r in seq]
        coords = [[float(p["stop_lon"]), float(p["stop_lat"])] for p in points]
        features = [
            {
                "geometry": {"coordinates": coords, "type": "LineString"},
                "properties": {"Route Code": route_id, "Travel Mode": "DRIVE"},
                "type": "Feature",
            }
        ]
        for i, (p, c) in enumerate(zip(points, coords), start=1):
            features.append(
                {
                    "geometry": {"coordinates": c, "type": "Point"},
                    "properties": {
                        "Provider Id": i,
                        "Stop Code": p["stop_code"] or p["stop_id"],
                        "Stop Name": p["stop_name"],
                        "Stop Type": "NEW STOP",
                        "routes": [route_id],
                    },
                    "type": "Feature",
                }
            )
        doc = {"features": features, "type": "FeatureCollection"}
        (out_dir / f"{route_id}.geojson").write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n")
        print(f"{route_id}: {len(points)} stops")


if __name__ == "__main__":
    main()
