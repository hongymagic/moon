import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  fullyParallel: true,
  forbidOnly: Boolean(process.env.CI),
  retries: 0,
  workers: 2,
  timeout: 20_000,
  outputDir: "../../target/settings-browser-tests",
  reporter: [
    ["list"],
    ["json", { outputFile: "../../target/settings-browser-results.json" }],
  ],
  use: {
    browserName: "chromium",
    headless: true,
    viewport: { width: 1200, height: 850 },
    trace: "retain-on-failure",
  },
});
