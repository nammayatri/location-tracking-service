const fs = require("fs");

// Function to create the ConfigMap
function generateConfigMap() {
  const configMap = {
    apiVersion: "v1",
    kind: "ConfigMap",
    metadata: {
      name: "location-tracking-service-bus-depot-geojson",
      namespace: "atlas",
    },
    data: {},
  };

  // Read all JSON files from the directory
  const depotDir = "../bus_depot_geo_config";
  const files = fs.readdirSync(depotDir);

  // Process each JSON file
  files.forEach((file) => {
    if (file.endsWith(".json")) {
      try {
        // Read the JSON file content
        const content = fs.readFileSync(`${depotDir}/${file}`, "utf8");
        // Add to ConfigMap data using the filename as key
        configMap.data[file] = content;
      } catch (err) {
        console.error(`Error processing ${file}:`, err);
      }
    }
  });

  // Convert to YAML format
  let yamlContent = "apiVersion: v1\n";
  yamlContent += "kind: ConfigMap\n";
  yamlContent += "metadata:\n";
  yamlContent += "  name: location-tracking-service-bus-depot-geojson\n";
  yamlContent += "  namespace: atlas\n";
  yamlContent += "data:\n";

  // Add each JSON file content with proper indentation
  Object.entries(configMap.data).forEach(([filename, content]) => {
    yamlContent += `  ${filename}: |-\n`;
    // Indent the JSON content
    content.split("\n").forEach((line) => {
      yamlContent += `    ${line}\n`;
    });
  });

  return yamlContent;
}

// Generate and save the ConfigMap
try {
  const yamlContent = generateConfigMap();
  fs.writeFileSync(
    "location-tracking-service-bus-depot-geojson.yaml",
    yamlContent
  );
  console.log("ConfigMap YAML file generated successfully!");
} catch (err) {
  console.error("Error generating ConfigMap:", err);
}
