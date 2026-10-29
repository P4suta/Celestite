import { listen } from "@tauri-apps/api/event";
import { expect, it, vi } from "vitest";
import { TauriBridge } from "../../src/lib/desktop";

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (): Promise<() => void> => () => {}),
}));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: (): { label: string } => ({ label: "note-second" }),
}));

it("subscribes to close requests for this window rather than every target", async () => {
  await new TauriBridge().onClose(() => {});
  expect(listen).toHaveBeenCalledWith("celestite:close-requested", expect.any(Function), {
    target: "note-second",
  });
});
