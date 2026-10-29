import { createHash } from "node:crypto";
import { expect, test } from "./fixtures";

const pngBytes = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x01, 0x02, 0x03];
const pngDigest = createHash("sha256").update(Uint8Array.from(pngBytes)).digest("hex");

async function paste(
  page: import("@playwright/test").Page,
  bytes: readonly number[],
  name: string,
  type: string,
): Promise<void> {
  await page.locator(".cm-content").evaluate(
    (target, file) => {
      const transfer = new DataTransfer();
      transfer.items.add(new File([Uint8Array.from(file.bytes)], file.name, { type: file.type }));
      target.dispatchEvent(
        new ClipboardEvent("paste", { clipboardData: transfer, bubbles: true, cancelable: true }),
      );
    },
    { bytes, name, type },
  );
}

test("pasting an image stores it by digest and inserts its reference as one undoable edit", async ({
  page,
}) => {
  await page.goto("/?fixture=library");
  const editor = page.getByRole("textbox", { name: "Markdown note" });
  await editor.click();
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.insertText("# Shots\n\n");
  await paste(page, pngBytes, "Screen Shot.png", "image/png");
  await expect(editor).toContainText("![Screen Shot]");
  await expect(editor).toContainText(`[Screen Shot]: assets/${pngDigest}.png`);
  await expect(page.locator("body")).toHaveAttribute("data-stored-asset", `${pngDigest}.png`);
  await page.keyboard.press("ControlOrMeta+z");
  await expect(editor).not.toContainText("![Screen Shot]");
  await expect(editor).not.toContainText("[Screen Shot]:");
  await expect(editor).toContainText("# Shots");
});

test("a natively dropped file lands at the caret and a failed drop shows a notice", async ({
  page,
}) => {
  await page.goto("/?fixture=library");
  const editor = page.getByRole("textbox", { name: "Markdown note" });
  await editor.click();
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.insertText("Before after");
  await page.keyboard.press("Home");
  for (let index = 0; index < "Before ".length; index += 1) await page.keyboard.press("ArrowRight");
  const digest = "c".repeat(64);
  await page.evaluate(
    ({ digest }) =>
      (
        window as unknown as {
          celestiteMock: { drop: (label: string, digest: string, extension: string) => void };
        }
      ).celestiteMock.drop("paper", digest, "pdf"),
    { digest },
  );
  await expect(editor).toContainText("Before ![paper]after");
  await expect(editor).toContainText(`[paper]: assets/${digest}.pdf`);
  await page.evaluate(() =>
    (
      window as unknown as { celestiteMock: { fail: (message: string) => void } }
    ).celestiteMock.fail("Save this note in the library before adding media."),
  );
  await expect(page.getByRole("alert")).toContainText("before adding media");
});

test("an unsupported paste keeps the note unchanged and explains why", async ({ page }) => {
  await page.goto("/?fixture=library");
  const editor = page.getByRole("textbox", { name: "Markdown note" });
  await editor.click();
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.insertText("Text");
  await paste(page, [0x4d, 0x5a, 0x90], "tool.exe", "application/octet-stream");
  await expect(page.getByRole("alert")).toContainText("not supported");
  await expect(editor).toContainText("Text");
  await expect(editor).not.toContainText("![");
  await page.getByRole("button", { name: "Dismiss" }).click();
  await expect(page.getByRole("alert")).toBeHidden();
});
