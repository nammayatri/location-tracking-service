// Global variables
let map;
let vehicleMarker;
let routePolyline;
let stopsMarkers = [];
let trackingInterval;
let currentRoute;
let etaList = [];
let currentPosition = 0;
let routeCoordinates = [];
let stops = [];
let isTracking = false;
let vehicleId = "WB052366";
let routeCode = "EB12-U";
let currentSpeed = 0;
let maxSpeed = 120; // Changed to 120 km/h
let nextStopIndex = 0;
let currentRideId = null; // Store the current ride ID
let driverId = "123";
let merchantId = "dev";
let lastUpdateTime = 0;
let distanceCovered = 0; // Track total distance covered
let previousETAs = new Map(); // Store previous ETAs for comparison
let etaChangeCount = new Map(); // Track number of significant changes
let etaTransitions = new Map(); // Track all ETA changes for each stop
let speedMultiplier = 1; // Speed multiplier for simulation
let pointSlider;
let pointValue;

// CSV simulation variables
let waypoints = [];
let currentWaypointIndex = 0;
let isCSVSimulation = false;
let simulationTimeout = null;

// Initialize the map
function initMap() {
  // Create map centered on Kolkata (where the route is located)
  map = L.map("map", {
    zoomControl: false,
    attributionControl: false,
  }).setView([22.5666, 88.31397], 13);

  // Add zoom control to top-right
  L.control
    .zoom({
      position: "topright",
    })
    .addTo(map);

  // Add attribution control to bottom-right
  L.control
    .attribution({
      position: "bottomright",
    })
    .addTo(map);

  // Add Voyager overlay with retina support
  L.tileLayer(
    "https://{s}.basemaps.cartocdn.com/rastertiles/voyager/{z}/{x}/{y}{r}.png",
    {
      attribution:
        '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors &copy; <a href="https://carto.com/attributions">CARTO</a>',
      subdomains: "abcd",
      maxZoom: 20,
      detectRetina: true,
    }
  ).addTo(map);

  // Create vehicle marker with custom icon
  const vehicleIcon = L.divIcon({
    className: "vehicle-icon",
    html: '<div class="vehicle-marker"></div>',
    iconSize: [20, 20],
    iconAnchor: [10, 10],
  });

  // Initialize vehicle marker with animation options
  vehicleMarker = L.marker([0, 0], {
    icon: vehicleIcon,
    smoothFactor: 1,
  }).addTo(map);

  // Initialize route polyline with gradient
  const gradient = {
    0.0: "#2563eb",
    0.5: "#357abd",
    1.0: "#2171cc",
  };

  routePolyline = L.polyline([], {
    color: "#2563eb",
    weight: 5,
    opacity: 0.9,
    lineCap: "round",
    lineJoin: "round",
    gradient: gradient,
  }).addTo(map);

  // Load route data from GeoJSON
  fetch("assets/route.geojson")
    .then((response) => response.json())
    .then((data) => {
      processRouteData(data);
    })
    .catch((error) => {
      console.error("Error loading route data:", error);
    });

  // Set up event listeners
  document.getElementById("start-btn").addEventListener("click", startTracking);
  document.getElementById("stop-btn").addEventListener("click", stopTracking);
  document.getElementById("reset-btn").addEventListener("click", resetTracking);

  // Update vehicle info
  document.getElementById("vehicle-id").textContent = vehicleId;
  document.getElementById("current-route").textContent = routeCode;
}

// Call initMap when the page loads
document.addEventListener("DOMContentLoaded", function () {
  initMap();

  // Set up speed multiplier control
  const multiplierSelect = document.getElementById("speed-multiplier");
  multiplierSelect.addEventListener("change", function () {
    speedMultiplier = parseInt(this.value);

    // Update interval if tracking is active
    if (isTracking && trackingInterval) {
      clearInterval(trackingInterval);
      const baseInterval = isCSVSimulation ? 2000 : 100;
      trackingInterval = setInterval(
        updateVehiclePosition,
        baseInterval / speedMultiplier
      );
    }
  });

  // Set up point slider
  pointSlider = document.getElementById("point-slider");
  pointValue = document.getElementById("point-value");

  pointSlider.addEventListener("input", function () {
    if (isCSVSimulation && waypoints.length > 0) {
      const index = parseInt(this.value);
      currentWaypointIndex = index;
      const waypoint = waypoints[index];

      // Update vehicle position
      vehicleMarker.setLatLng([waypoint.lat, waypoint.lng]);

      // Update speed display
      currentSpeed = waypoint.speed * 3.6;
      document.getElementById(
        "current-speed"
      ).textContent = `${currentSpeed.toFixed(1)} km/h`;

      // Calculate heading if not at last point
      if (index < waypoints.length - 1) {
        const nextWaypoint = waypoints[index + 1];
        const heading = calculateHeading(
          waypoint.lng,
          waypoint.lat,
          nextWaypoint.lng,
          nextWaypoint.lat
        );
        const markerElement = vehicleMarker.getElement();
        if (markerElement) {
          const marker = markerElement.querySelector(".vehicle-marker");
          if (marker) {
            marker.style.transform = `rotate(${heading}deg)`;
          }
        }
      }

      // Update APIs
      updateLocationAPI(waypoint.lat, waypoint.lng);
      getEtaUpdatesAPI(waypoint.lat, waypoint.lng);
    }
  });
});

// Process route data from GeoJSON
function processRouteData(data) {
  // Extract route coordinates and stops
  const routeFeature = data.features.find(
    (feature) => feature.geometry.type === "LineString"
  );
  const stopFeatures = data.features.filter(
    (feature) => feature.geometry.type === "Point"
  );

  if (routeFeature) {
    routeCoordinates = routeFeature.geometry.coordinates.map((coord) => [
      coord[1],
      coord[0],
    ]);
    routePolyline.setLatLngs(routeCoordinates);
    map.fitBounds(routePolyline.getBounds());

    // Set initial vehicle position
    if (routeCoordinates.length > 0) {
      vehicleMarker.setLatLng(routeCoordinates[0]);
    }
  }

  // Process stops
  stops = stopFeatures.map((feature, index) => {
    const coords = feature.geometry.coordinates;
    const props = feature.properties;

    return {
      lat: coords[1],
      lng: coords[0],
      name: props["Stop Name"],
      code: props["Stop Code"],
      type: props["Stop Type"],
      sequence: index + 1,
    };
  });

  // Add stop markers to map
  stops.forEach((stop, index) => {
    const stopIcon = L.divIcon({
      className: "stop-icon",
      html: `<div class="stop-marker"><span class="stop-number">${
        index + 1
      }</span></div>`,
      iconSize: [24, 24],
      iconAnchor: [12, 12],
    });

    const marker = L.marker([stop.lat, stop.lng], { icon: stopIcon })
      .bindTooltip(`<b>${stop.name}</b><br>Stop #${index + 1}`, {
        permanent: false,
        direction: "top",
        offset: L.point(0, -10),
        className: "stop-tooltip",
      })
      .addTo(map);

    stopsMarkers.push(marker);
  });

  // Initialize ETA list
  updateEtaList();
}

// Start tracking
function startTracking() {
  if (!isTracking) {
    isTracking = true;
    document.getElementById("start-btn").disabled = true;
    document.getElementById("stop-btn").disabled = false;

    // Initialize ride with API calls
    initializeRide()
      .then(() => {
        // Update interval based on simulation type and speed multiplier
        const baseInterval = isCSVSimulation ? 2000 : 100;
        trackingInterval = setInterval(
          updateVehiclePosition,
          baseInterval / speedMultiplier
        );
      })
      .catch((error) => {
        console.error("Failed to initialize ride:", error);
        isTracking = false;
        document.getElementById("start-btn").disabled = false;
        document.getElementById("stop-btn").disabled = true;
      });
  }
}

// Initialize ride with API calls
async function initializeRide() {
  try {
    // Get the first stop coordinates (start of route)
    const startStop = stops[0];
    const endStop = stops[stops.length - 1];

    // Generate a unique ride ID
    currentRideId = `test__${Date.now()}`;

    // Call ride details API
    await createRideDetails(currentRideId, startStop, endStop);

    // Call start ride API
    await startRide(currentRideId, startStop, endStop);

    console.log("Ride initialized successfully");
    return true;
  } catch (error) {
    console.error("Error initializing ride:", error);
    throw error;
  }
}

// Create ride details
function createRideDetails(rideId, startStop, endStop) {
  const payload = {
    rideId: rideId,
    rideStatus: "NEW",
    lat: startStop.lat,
    lon: startStop.lng,
    driverId,
    merchantId,
    rideInfo: {
      bus: {
        routeCode: routeCode,
        busNumber: vehicleId,
        destination: {
          lat: endStop.lat,
          lon: endStop.lng,
        },
      },
    },
  };

  return fetch("http://localhost:8081/internal/ride/rideDetails", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify(payload),
  })
    .then((response) => {
      if (!response.ok) {
        throw new Error(`HTTP error! Status: ${response.status}`);
      }
      return response.json();
    })
    .then((data) => {
      console.log("Ride details created:", data);
      return data;
    });
}

// Start ride
function startRide(rideId, startStop, endStop) {
  const payload = {
    lat: startStop.lat,
    lon: startStop.lng,
    driverId,
    merchantId,
    rideInfo: {
      bus: {
        routeCode: routeCode,
        busNumber: vehicleId,
        destination: {
          lat: endStop.lat,
          lon: endStop.lng,
        },
      },
    },
  };

  return fetch(`http://localhost:8081/internal/ride/${rideId}/start`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify(payload),
  })
    .then((response) => {
      if (!response.ok) {
        throw new Error(`HTTP error! Status: ${response.status}`);
      }
      return response.json();
    })
    .then((data) => {
      console.log("Ride started:", data);
      return data;
    });
}

// Stop tracking
function stopTracking() {
  if (isTracking) {
    isTracking = false;
    document.getElementById("start-btn").disabled = false;
    document.getElementById("stop-btn").disabled = true;

    // Stop updating vehicle position
    clearInterval(trackingInterval);
  }
}

// End ride
function endRide(currentLat, currentLng, endStop) {
  if (!currentRideId) {
    console.error("No active ride to end");
    return Promise.reject(new Error("No active ride to end"));
  }

  const payload = {
    lat: currentLat,
    lon: currentLng,
    driverId,
    merchantId,
    rideInfo: {
      bus: {
        routeCode: routeCode,
        busNumber: vehicleId,
        destination: {
          lat: endStop.lat,
          lon: endStop.lng,
        },
      },
    },
  };

  return fetch(`http://localhost:8081/internal/ride/${currentRideId}/end`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify(payload),
  })
    .then((response) => {
      if (!response.ok) {
        throw new Error(`HTTP error! Status: ${response.status}`);
      }
      return response.json();
    })
    .finally(() => {
      // Clear the ride ID after ending
      currentRideId = null;
    });
}

// Reset tracking
function resetTracking() {
  stopTracking();
  distanceCovered = 0;
  currentSpeed = 0;
  previousETAs.clear(); // Clear stored ETAs
  etaChangeCount.clear(); // Clear change counts
  etaTransitions.clear(); // Clear transition history

  if (routeCoordinates.length > 0) {
    vehicleMarker.setLatLng([routeCoordinates[0][0], routeCoordinates[0][1]]);
  }

  // Reset all stop markers to their original state
  stopsMarkers.forEach((marker, index) => {
    const markerElement = marker.getElement();
    if (markerElement) {
      const stopMarker = markerElement.querySelector(".stop-marker");
      if (stopMarker) {
        stopMarker.classList.remove("reached");
      }
    }
    // Reset tooltip to show only stop name
    marker.setTooltipContent(
      `<b>${stops[index].name}</b><br>Stop #${index + 1}`
    );
  });

  // Update UI
  document.getElementById("current-speed").textContent = "0 km/h";
  document.getElementById("next-stop").textContent = "N/A";

  // Update ETA list
  updateEtaList();

  // Get current position and end stop
  const currentLat = vehicleMarker.getLatLng().lat;
  const currentLng = vehicleMarker.getLatLng().lng;
  const endStop = stops[stops.length - 1];

  // End the ride
  endRide(currentLat, currentLng, endStop)
    .then(() => {
      console.log("Ride ended successfully");
    })
    .catch((error) => {
      console.error("Failed to end ride:", error);
    });

  // Reset point slider
  if (isCSVSimulation) {
    currentWaypointIndex = 0;
    pointSlider.value = 0;
    pointValue.textContent = `0/${waypoints.length - 1}`;
  }
}

// Update vehicle position
function updateVehiclePosition() {
  if (!isTracking) return;

  if (isCSVSimulation) {
    if (currentWaypointIndex >= waypoints.length) {
      stopTracking();
      return;
    }

    const waypoint = waypoints[currentWaypointIndex];

    // Update vehicle position
    vehicleMarker.setLatLng([waypoint.lat, waypoint.lng]);

    // Update speed display (convert m/s to km/h)
    currentSpeed = waypoint.speed * 3.6;
    document.getElementById(
      "current-speed"
    ).textContent = `${currentSpeed.toFixed(1)} km/h`;

    // Calculate heading for vehicle rotation
    if (currentWaypointIndex < waypoints.length - 1) {
      const nextWaypoint = waypoints[currentWaypointIndex + 1];
      const heading = calculateHeading(
        waypoint.lng,
        waypoint.lat,
        nextWaypoint.lng,
        nextWaypoint.lat
      );
      const markerElement = vehicleMarker.getElement();
      if (markerElement) {
        const marker = markerElement.querySelector(".vehicle-marker");
        if (marker) {
          marker.style.transform = `rotate(${heading}deg)`;
        }
      }
    }

    // Update APIs
    updateLocationAPI(waypoint.lat, waypoint.lng);
    getEtaUpdatesAPI(waypoint.lat, waypoint.lng);

    // Update point slider
    pointSlider.value = currentWaypointIndex;
    pointValue.textContent = `${currentWaypointIndex}/${waypoints.length - 1}`;

    currentWaypointIndex++;
    return;
  }

  if (routeCoordinates.length === 0) return;

  // Speed: 120 km/h = 5.56 m/s
  currentSpeed = 120;
  const speedMS = (currentSpeed * 1000) / 3600; // Convert 20 km/h to m/s

  // Calculate distance to move in this frame (100ms)
  const timeStep = 0.1; // 100ms = 0.1s
  const distanceThisFrame = speedMS * timeStep; // Distance in meters

  // Update total distance covered
  distanceCovered += distanceThisFrame;

  // Find current position on route
  let totalDistance = 0;
  let currentIndex = 0;
  let fraction = 0;

  // Find the current segment based on distance covered
  for (let i = 0; i < routeCoordinates.length - 1; i++) {
    const p1 = routeCoordinates[i];
    const p2 = routeCoordinates[i + 1];

    const segmentDistance = calculateDistanceInMeters(
      p1[0],
      p1[1],
      p2[0],
      p2[1]
    );

    if (totalDistance + segmentDistance > distanceCovered) {
      currentIndex = i;
      fraction = (distanceCovered - totalDistance) / segmentDistance;
      break;
    }

    totalDistance += segmentDistance;
  }

  // Reset if we've reached the end of the route
  if (currentIndex >= routeCoordinates.length - 1) {
    distanceCovered = 0;
    currentIndex = 0;
    fraction = 0;
  }

  // Calculate interpolated position
  const point1 = routeCoordinates[currentIndex];
  const point2 = routeCoordinates[currentIndex + 1];

  const lat = point1[0] + (point2[0] - point1[0]) * fraction;
  const lng = point1[1] + (point2[1] - point1[1]) * fraction;

  // Calculate heading
  const heading = calculateHeading(point1[1], point1[0], point2[1], point2[0]);

  // Update marker rotation
  const markerElement = vehicleMarker.getElement();
  if (markerElement) {
    const marker = markerElement.querySelector(".vehicle-marker");
    if (marker) {
      marker.style.transform = `rotate(${heading}deg)`;
    }
  }

  // Update marker position
  vehicleMarker.setLatLng([lat, lng], {
    animate: true,
    duration: 100,
  });

  // Update UI
  document.getElementById(
    "current-speed"
  ).textContent = `${currentSpeed.toFixed(1)} km/h`;

  // Call APIs every second
  if (Math.floor(Date.now() / 1000) !== lastUpdateTime) {
    updateLocationAPI(lat, lng);
    getEtaUpdatesAPI(lat, lng);
    lastUpdateTime = Math.floor(Date.now() / 1000);
  }
}

// Calculate distance between two points in meters
function calculateDistanceInMeters(lat1, lon1, lat2, lon2) {
  const R = 6371000; // Earth's radius in meters
  const dLat = deg2rad(lat2 - lat1);
  const dLon = deg2rad(lon2 - lon1);
  const a =
    Math.sin(dLat / 2) * Math.sin(dLat / 2) +
    Math.cos(deg2rad(lat1)) *
      Math.cos(deg2rad(lat2)) *
      Math.sin(dLon / 2) *
      Math.sin(dLon / 2);
  const c = 2 * Math.atan2(Math.sqrt(a), Math.sqrt(1 - a));
  return R * c; // Distance in meters
}

// Calculate heading between two points in degrees
function calculateHeading(lon1, lat1, lon2, lat2) {
  const dLon = deg2rad(lon2 - lon1);
  const y = Math.sin(dLon) * Math.cos(deg2rad(lat2));
  const x =
    Math.cos(deg2rad(lat1)) * Math.sin(deg2rad(lat2)) -
    Math.sin(deg2rad(lat1)) * Math.cos(deg2rad(lat2)) * Math.cos(dLon);
  let heading = rad2deg(Math.atan2(y, x));
  heading = (heading + 360) % 360; // Normalize to 0-360
  return heading;
}

// Convert degrees to radians
function deg2rad(deg) {
  return deg * (Math.PI / 180);
}

// Convert radians to degrees
function rad2deg(rad) {
  return rad * (180 / Math.PI);
}

// Update ETA list
function updateEtaList() {
  const etaListElement = document.getElementById("eta-list");
  etaListElement.innerHTML = "";

  if (stops.length === 0) return;

  // Create ETA items for each stop
  stops.forEach((stop, index) => {
    const etaItem = document.createElement("div");
    etaItem.className = "eta-item";

    const stopName = document.createElement("div");
    stopName.className = "stop-name";
    stopName.textContent = stop.name;

    const etaTime = document.createElement("div");
    etaTime.className = "eta-time";
    etaTime.id = `eta-${index}`;
    etaTime.textContent = "Calculating...";

    etaItem.appendChild(stopName);
    etaItem.appendChild(etaTime);
    etaListElement.appendChild(etaItem);
  });
}

// Simulate API call to update location
function updateLocationAPI(lat, lng) {
  // Convert speed from km/h to m/s
  const speedInMS = (currentSpeed * 1000) / 3600; // Convert 20 km/h to m/s

  // Create the payload
  const payload = [
    {
      pt: {
        lat: lat,
        lon: lng,
      },
      ts: new Date().toISOString(),
      acc: 0,
      v: speedInMS, // Speed in m/s
    },
  ];

  // Make the API call
  fetch("http://localhost:8081/ui/driver/location", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      token: "123",
      vt: "BUS_AC",
      mId: "dev",
      dm: "ONLINE",
    },
    body: JSON.stringify(payload),
  })
    .then((response) => {
      if (!response.ok) {
        throw new Error(`HTTP error! Status: ${response.status}`);
      }
      return response.status;
    })
    .then((data) => {
      console.log("Location update successful:", data);
    })
    .catch((error) => {
      console.error("Error updating location:", error);
    });
}

// Simulate API call to get ETA updates
function getEtaUpdatesAPI(lat, lng) {
  // Create the payload
  const payload = {
    routeCode: routeCode,
  };

  // Make the API call
  fetch("http://localhost:8081/internal/trackVehicles", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      token: "123",
    },
    body: JSON.stringify(payload),
  })
    .then((response) => {
      if (!response.ok) {
        throw new Error(`HTTP error! Status: ${response.status}`);
      }
      return response.json();
    })
    .then((data) => {
      console.log("ETA update successful:", data);

      // Process the ETA data
      let vehicleData = data.find((item) => item.vehicleNumber === vehicleId);

      if (vehicleData && vehicleData.vehicleInfo) {
        // Update next stop information from API response
        let nextStop = vehicleData.vehicleInfo.upcomingStops.find(
          (upcomingStop) => upcomingStop.status === "Upcoming"
        );
        if (nextStop) {
          document.getElementById("current-speed").textContent = currentSpeed
            ? `${currentSpeed.toFixed(1)} km/h`
            : "0 km/h";
          document.getElementById("next-stop").textContent =
            nextStop.stop.name || "N/A";
        }

        // Update ETA list if upcomingStops is available
        if (vehicleData.vehicleInfo.upcomingStops) {
          updateEtaListWithApiData(vehicleData.vehicleInfo.upcomingStops);
        }
      }
    })
    .catch((error) => {
      console.error("Error getting ETA updates:", error);
    });
}

// Update ETA list with data from API
function updateEtaListWithApiData(upcomingStops) {
  const etaListElement = document.getElementById("eta-list");
  etaListElement.innerHTML = "";

  if (!upcomingStops || upcomingStops.length === 0) return;

  // Separate reached and upcoming stops
  const reachedStops = upcomingStops.filter(
    (stop) => stop.status === "Reached"
  );
  const pendingStops = upcomingStops.filter(
    (stop) => stop.status !== "Reached"
  );

  // Create Reached Stops section if there are any reached stops
  if (reachedStops.length > 0) {
    const reachedSection = document.createElement("div");
    reachedSection.className = "stops-section reached-section";

    const reachedHeader = document.createElement("h3");
    reachedHeader.className = "section-header";
    reachedHeader.textContent = "Reached Stops";
    reachedSection.appendChild(reachedHeader);

    reachedStops.forEach((stop) => createStopItem(stop, reachedSection));
    etaListElement.appendChild(reachedSection);
  }

  // Create Upcoming Stops section
  const upcomingSection = document.createElement("div");
  upcomingSection.className = "stops-section upcoming-section";

  const upcomingHeader = document.createElement("h3");
  upcomingHeader.className = "section-header";
  upcomingHeader.textContent = "Upcoming Stops";
  upcomingSection.appendChild(upcomingHeader);

  pendingStops.forEach((stop) => createStopItem(stop, upcomingSection));
  etaListElement.appendChild(upcomingSection);
}

// Helper function to create a stop item
function createStopItem(stopData, parentElement) {
  const { eta, delta, stop, status } = stopData;
  const etaItem = document.createElement("div");
  etaItem.className = `eta-item ${status.toLowerCase()}`;

  const stopName = document.createElement("div");
  stopName.className = "stop-name";
  stopName.textContent = stop.name;

  const timeInfo = document.createElement("div");
  timeInfo.className = "time-info";

  const statusLabel = document.createElement("span");
  statusLabel.className = "status-label";

  const timeDisplay = document.createElement("span");
  timeDisplay.className = "eta-time";

  // Calculate and track ETA changes
  const stopKey = `${stop.coordinate.lat}-${stop.coordinate.lon}`;
  let timeChangeMinutes = 0;
  let isDelayed = false;

  // Format time based on status
  const etaDate = new Date(eta);
  const deltaMs = delta * 1000;
  const finalEtaTime = new Date(etaDate.getTime() + deltaMs);

  if (status === "Reached") {
    statusLabel.textContent = "Reached at ";

    if (!isNaN(finalEtaTime.getTime())) {
      const hours = finalEtaTime.getHours();
      const minutes = finalEtaTime.getMinutes();
      const ampm = hours >= 12 ? "PM" : "AM";
      const formattedHours = hours % 12 || 12;
      const formattedMinutes = minutes.toString().padStart(2, "0");
      timeDisplay.textContent = `${formattedHours}:${formattedMinutes} ${ampm}`;
    } else {
      timeDisplay.textContent = "Time not available";
    }
  } else {
    statusLabel.textContent = "ETA: ";

    if (previousETAs.has(stopKey)) {
      const prevEta = previousETAs.get(stopKey);
      // Compare minutes directly instead of calculating time difference
      const prevMinutes = prevEta.getHours() * 60 + prevEta.getMinutes();
      const currentMinutes =
        finalEtaTime.getHours() * 60 + finalEtaTime.getMinutes();
      const minutesDiff = currentMinutes - prevMinutes;

      if (minutesDiff !== 0) {
        // Track the transition
        if (!etaTransitions.has(stopKey)) {
          etaTransitions.set(stopKey, []);
        }
        etaTransitions.get(stopKey).push({
          prevEta: prevEta,
          newEta: finalEtaTime,
          diffMinutes: minutesDiff,
          timestamp: new Date(),
        });

        timeChangeMinutes = minutesDiff;
        isDelayed = minutesDiff > 0;
        previousETAs.set(stopKey, finalEtaTime);
      }
    } else {
      previousETAs.set(stopKey, finalEtaTime);
    }

    const hours = finalEtaTime.getHours();
    const minutes = finalEtaTime.getMinutes();
    const ampm = hours >= 12 ? "PM" : "AM";
    const formattedHours = hours % 12 || 12;
    const formattedMinutes = minutes.toString().padStart(2, "0");
    timeDisplay.textContent = `${formattedHours}:${formattedMinutes} ${ampm}`;
  }

  timeInfo.appendChild(statusLabel);
  timeInfo.appendChild(timeDisplay);
  etaItem.appendChild(stopName);
  etaItem.appendChild(timeInfo);

  // Add change count indicator if there were changes
  if (etaChangeCount.has(stopKey) && status !== "Reached") {
    const changeIndicator = document.createElement("div");
    changeIndicator.className = "eta-changes";
    const count = etaChangeCount.get(stopKey);
    changeIndicator.title = `ETA updated ${count} time${
      count !== 1 ? "s" : ""
    } by more than 1 minute`;
    etaItem.appendChild(changeIndicator);
  }

  parentElement.appendChild(etaItem);

  // Update marker tooltip
  const stopMarker = stopsMarkers.find((marker) => {
    const latLng = marker.getLatLng();
    return (
      Math.abs(latLng.lat - stop.coordinate.lat) < 0.0001 &&
      Math.abs(latLng.lng - stop.coordinate.lon) < 0.0001
    );
  });

  if (stopMarker) {
    const tooltipContent =
      status === "Reached"
        ? `<b>${stop.name}</b><br>Reached at ${timeDisplay.textContent}`
        : `<b>${stop.name}</b><br>ETA: ${timeDisplay.textContent}`;

    stopMarker.setTooltipContent(tooltipContent);

    // Update marker style based on status
    const markerElement = stopMarker.getElement();
    if (markerElement) {
      const marker = markerElement.querySelector(".stop-marker");
      if (marker) {
        marker.classList.toggle("reached", status === "Reached");
      }
    }
  }

  // Add click handler for ETA history popup
  etaItem.style.cursor = "pointer";
  etaItem.addEventListener("click", () => {
    const transitions = etaTransitions.get(stopKey) || [];
    let delayedCount = 0;
    let earlierCount = 0;

    transitions.forEach((t) => {
      if (t.diffMinutes > 0) delayedCount++;
      if (t.diffMinutes < 0) earlierCount++;
    });

    const formatTime = (date) => {
      return date.toLocaleTimeString("en-US", {
        hour: "numeric",
        minute: "2-digit",
        hour12: true,
      });
    };

    let popupContent = `
        <div style="padding: 10px; max-height: 200px; overflow-y: auto;">
          <h3 style="margin-bottom: 8px; color: #2c3e50;">${
            stop.name
          } - ETA Changes</h3>
          <div style="margin-bottom: 8px; color: #64748b;">
            Total Delays: ${delayedCount} | Earlier Arrivals: ${earlierCount}
          </div>
          <div style="border-top: 1px solid #e2e8f0; padding-top: 8px;">
            ${transitions
              .map(
                (t) => `
              <div style="margin-bottom: 6px; font-size: 13px;">
                <span style="color: ${
                  t.diffMinutes > 0 ? "#ef4444" : "#22c55e"
                }">
                  ${t.diffMinutes > 0 ? "⬆" : "⬇"} ${Math.abs(
                  t.diffMinutes
                )} min
                </span>
                <br>
                <span style="color: #64748b;">
                  ${formatTime(t.prevEta)} → ${formatTime(t.newEta)}
                </span>
              </div>
            `
              )
              .join("")}
          </div>
        </div>
      `;

    // Create and show popup
    const popup = L.popup({
      maxWidth: 300,
      className: "eta-history-popup",
    })
      .setLatLng([stop.coordinate.lat, stop.coordinate.lon])
      .setContent(popupContent)
      .openOn(map);
  });
}

// CSV upload and processing
document.addEventListener("DOMContentLoaded", function () {
  const csvUpload = document.getElementById("csv-upload");
  const uploadStatus = document.getElementById("upload-status");

  csvUpload.addEventListener("change", function (e) {
    const file = e.target.files[0];
    if (file) {
      const reader = new FileReader();
      reader.onload = function (e) {
        try {
          processCSV(e.target.result);
          uploadStatus.textContent =
            "CSV loaded successfully! Click Start Tracking to begin simulation.";
          uploadStatus.className = "upload-status success";
        } catch (error) {
          uploadStatus.textContent = "Error processing CSV: " + error.message;
          uploadStatus.className = "upload-status error";
        }
      };
      reader.onerror = function () {
        uploadStatus.textContent = "Error reading file";
        uploadStatus.className = "upload-status error";
      };
      reader.readAsText(file);
    }
  });
});

function processCSV(csvContent) {
  // Reset simulation state
  waypoints = [];
  currentWaypointIndex = 0;
  isCSVSimulation = false;
  if (simulationTimeout) {
    clearTimeout(simulationTimeout);
  }

  // Parse CSV
  const lines = csvContent.split("\n");
  const headers = lines[0].split(",");

  // Validate headers
  const requiredHeaders = ["Driver ID", "Rid", "Ts", "Lat", "Lon", "Speed"];
  const headerValid = requiredHeaders.every((h) => headers.includes(h));

  if (!headerValid) {
    throw new Error(
      "Invalid CSV format. Required columns: " + requiredHeaders.join(", ")
    );
  }

  // Parse waypoints
  for (let i = 1; i < lines.length; i++) {
    const line = lines[i].trim();
    if (line) {
      const values = line.split(",");
      waypoints.push({
        driverId: values[0],
        rideId: values[1],
        ts: new Date(values[2]), // Keep original timestamp
        lat: parseFloat(values[3]),
        lng: parseFloat(values[4]),
        speed: parseFloat(values[5]), // Speed in m/s
      });
    }
  }

  if (waypoints.length === 0) {
    throw new Error("No valid waypoints found in CSV");
  }

  // Sort waypoints by timestamp
  waypoints.sort((a, b) => a.ts - b.ts);

  // Update point slider range
  pointSlider.min = 0;
  pointSlider.max = waypoints.length - 1;
  pointSlider.value = 0;
  pointSlider.disabled = false;
  pointValue.textContent = `0/${waypoints.length - 1}`;

  // Set initial vehicle position
  if (waypoints.length > 0) {
    const firstWaypoint = waypoints[0];
    vehicleMarker.setLatLng([firstWaypoint.lat, firstWaypoint.lng]);
    currentRideId = firstWaypoint.rideId;
    map.setView([firstWaypoint.lat, firstWaypoint.lng], 13);
  }

  isCSVSimulation = true;
}
