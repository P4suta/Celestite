import { defineConfig } from "@playwright/test";

const DEV_SERVER_URL = "http://127.0.0.1:1420";
const DESKTOP_VIEWPORT_WIDTH = 1280;
const DESKTOP_VIEWPORT_HEIGHT = 800;

export default defineConfig({
  testDir: "tests/browser",
  fullyParallel: false,
  workers: 1,
  retries: 0,
  projects: [
    { name: "chromium", use: { browserName: "chromium" } },
    { name: "webkit", use: { browserName: "webkit" } },
  ],
  use: {
    baseURL: DEV_SERVER_URL,
    viewport: { width: DESKTOP_VIEWPORT_WIDTH, height: DESKTOP_VIEWPORT_HEIGHT },
    trace: "retain-on-failure",
  },
  webServer: {
    command: "pnpm dev --mode test",
    url: DEV_SERVER_URL,
    reuseExistingServer: false,
  },
});
