import { expect, test } from "./fixtures";

test("writing hides chrome and keeps the caret centered after many lines", async ({ page }) => {
  await page.goto("/");
  const editor = page.getByRole("textbox", { name: "Markdown note" });
  await expect(editor).toBeVisible();
  await editor.fill("# A quiet place\n\nWrite in plain Markdown.");
  await expect(page.getByRole("button", { name: "File menu" })).toBeHidden();
  for (let line = 0; line < 20; line++) {
    await editor.press("End");
    await editor.press("Enter");
    await page.keyboard.insertText(`Line ${line}`);
  }
  await expect
    .poll(async () => {
      const caret = await page.locator(".cm-cursor-primary").boundingBox();
      return caret ? Math.abs(caret.y + caret.height / 2 - 400) : 100;
    })
    .toBeLessThan(3);
  await page.setViewportSize({ width: 640, height: 600 });
  await expect
    .poll(async () => {
      const caret = await page.locator(".cm-cursor-primary").boundingBox();
      return caret ? Math.abs(caret.y + caret.height / 2 - 300) : 100;
    })
    .toBeLessThan(3);
  await page.keyboard.press("Escape");
  await expect(page.getByRole("button", { name: "File menu" })).toBeVisible();
});

test("menu groups, keyboard navigation, and monochrome appearance work", async ({ page }) => {
  await page.goto("/?fixture=markdown");
  await page.getByRole("button", { name: "File menu" }).click();
  await expect(page.getByRole("separator")).toHaveCount(3);
  await expect(page.getByRole("menuitem", { name: "New" })).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(page.getByRole("menuitem", { name: "Open…", exact: false })).toBeFocused();
  await page.getByRole("menuitemradio", { name: "Dark" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-appearance", "dark");
  await expect(page.getByRole("menuitemradio", { name: "Dark" })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await page.keyboard.press("Escape");
  await expect(page.getByRole("menu")).toBeHidden();
});

test("source stays visible, emphasis is bold, and Find navigates matches", async ({ page }) => {
  await page.goto("/?fixture=markdown");
  const editor = page.getByRole("textbox", { name: "Markdown note" });
  await expect(editor).toContainText("**visible**");
  await expect(editor.getByText("visible", { exact: true })).toHaveCSS("font-weight", "700");
  await page.keyboard.press("ControlOrMeta+f");
  const search = page.getByRole("textbox", { name: "Find", exact: true });
  await search.fill("a");
  await expect(page.locator("output")).not.toHaveText("0 / 0");
  await expect(search).toBeFocused();
  await page.getByRole("button", { name: "Next match" }).click();
  await page.getByRole("button", { name: "Close find" }).click();
  await expect(editor).toBeFocused();
});

test("an untitled note requires Save, Don’t Save, or Cancel before close", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("textbox", { name: "Markdown note" }).fill("Keep this draft.");
  await page.keyboard.press("ControlOrMeta+w");
  await expect(page.getByRole("dialog")).toBeVisible();
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(page.getByRole("dialog")).toBeHidden();
  await expect(page.getByRole("textbox", { name: "Markdown note" })).toBeFocused();
  await expect(page.getByRole("textbox", { name: "Markdown note" })).toContainText(
    "Keep this draft.",
  );
  await page.keyboard.press("ControlOrMeta+w");
  await page.getByRole("button", { name: "Don’t Save", exact: true }).click();
  await expect(page.locator("body")).toHaveAttribute("data-closed", "true");
});

test("Save As returns focus to the editor so writing can continue", async ({ page }) => {
  await page.goto("/");
  const filename = page.getByRole("button", { name: "File menu" });
  const filenameText = filename.locator("span").first();
  await expect(filenameText).toHaveText(/^[0-9a-f-]+\.md$/u);
  const generatedName = (await filenameText.textContent())?.trim();
  if (!generatedName) throw new Error("The generated filename is missing.");
  const editor = page.getByRole("textbox", { name: "Markdown note" });
  await editor.fill("Write, save, keep writing.");
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "File menu" }).click();
  await page.getByRole("menuitem", { name: "Save As…", exact: false }).click();
  await expect(page.locator("body")).toHaveAttribute("data-save-default", generatedName);
  await expect(filename).toContainText(generatedName);
  await expect(editor).toBeFocused();
  await page.keyboard.insertText(" Still writing.");
  await expect(editor).toContainText("Still writing.");
});
