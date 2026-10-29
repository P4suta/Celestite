import type { Page } from "@playwright/test";
import { expect, test } from "./fixtures";

const SAMPLED_FRAMES = 8;
const MAX_CARET_DRIFT_PX = 2;

interface CaretFrame {
  readonly center: number;
  readonly scrollTop: number;
}

interface CaretTrace {
  readonly baseline: number;
  readonly target: number;
  readonly scrollTop: number;
  readonly frames: readonly CaretFrame[];
}

async function captureKeyFrames(page: Page, key: string): Promise<CaretTrace> {
  const probe = await page.evaluateHandle((sampleCount) => {
    const content = document.querySelector(".cm-content");
    const cursor = document.querySelector(".cm-cursor-primary");
    const scroller = document.querySelector(".cm-scroller");
    if (!content || !cursor || !scroller) throw new Error("The editor is not ready.");
    const bounds = cursor.getBoundingClientRect();
    const baseline = bounds.top + bounds.height / 2;
    const pending = new Promise<readonly CaretFrame[]>((resolve, reject) => {
      content.addEventListener(
        "keydown",
        () => {
          const frames: CaretFrame[] = [];
          const channel = new MessageChannel();
          function schedule(): void {
            requestAnimationFrame(() => channel.port2.postMessage(null));
          }
          function close(): void {
            channel.port1.close();
            channel.port2.close();
          }
          channel.port1.onmessage = (): void => {
            const currentCursor = document.querySelector(".cm-cursor-primary");
            if (!currentCursor) {
              close();
              reject(new Error("The caret disappeared during editing."));
              return;
            }
            const caret = currentCursor.getBoundingClientRect();
            frames.push({ center: caret.top + caret.height / 2, scrollTop: scroller.scrollTop });
            if (frames.length === sampleCount) {
              close();
              resolve(frames);
            } else schedule();
          };
          schedule();
        },
        { capture: true, once: true },
      );
    });
    return { baseline, target: window.innerHeight / 2, scrollTop: scroller.scrollTop, pending };
  }, SAMPLED_FRAMES);
  try {
    await page.getByRole("textbox", { name: "Markdown note" }).press(key);
    return await probe.evaluate(async ({ baseline, target, scrollTop, pending }) => ({
      baseline,
      target,
      scrollTop,
      frames: await pending,
    }));
  } finally {
    await probe.dispose();
  }
}

const documents = [
  { name: "short lines", text: "First line.\nSecond line.\nThird line.\n" },
  {
    name: "wrapped Japanese lines",
    text: `最初の段落です。\n${"日本語の文章を折り返しながら、書く位置を確認します。".repeat(18)}\n`,
  },
];

for (const fixture of documents) {
  for (const key of ["Enter", "Backspace", "ArrowUp", "ArrowDown"]) {
    test(`keeps the writing line fixed throughout ${key} with ${fixture.name}`, async ({
      page,
    }) => {
      await page.goto("/");
      const editor = page.getByRole("textbox", { name: "Markdown note" });
      await editor.fill(fixture.text);
      if (key === "ArrowDown") await editor.press("ArrowUp");
      await page.evaluate(async () => {
        await document.fonts.ready;
        await new Promise<void>((resolve) =>
          requestAnimationFrame(() =>
            requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
          ),
        );
      });
      const trace = await captureKeyFrames(page, key);
      await test.info().attach("caret-frames.json", {
        body: JSON.stringify(trace, null, 2),
        contentType: "application/json",
      });
      expect(Math.abs(trace.baseline - trace.target), JSON.stringify(trace)).toBeLessThanOrEqual(
        MAX_CARET_DRIFT_PX,
      );
      expect(
        Math.max(...trace.frames.map((frame) => Math.abs(frame.center - trace.target))),
        JSON.stringify(trace),
      ).toBeLessThanOrEqual(MAX_CARET_DRIFT_PX);
      expect(
        trace.frames.some(
          (frame) => Math.abs(frame.scrollTop - trace.scrollTop) > MAX_CARET_DRIFT_PX,
        ),
        JSON.stringify(trace),
      ).toBe(true);
    });
  }
}
