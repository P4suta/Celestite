import { expect, test } from "./fixtures";

test("ordinary clicks edit links and modifier-click jumps to a centered heading", async ({
  page,
}) => {
  await page.goto("/?fixture=links");
  const editor = page.getByRole("textbox", { name: "Markdown note" });
  const link = editor.locator(".cm-note-link").filter({ hasText: "[here]" });
  await expect(link).toBeVisible();
  const before = (await editor.textContent()) ?? "";
  await link.click();
  await expect(page.locator("body")).not.toHaveAttribute("data-link-action");
  await link.click({ modifiers: ["ControlOrMeta"] });
  await expect(page.locator("body")).toHaveAttribute("data-link-action", "heading");
  await expect(editor).toHaveText(before);
  await expect
    .poll(() =>
      page.evaluate(
        () => window.getSelection()?.anchorNode?.parentElement?.closest(".cm-line")?.textContent,
      ),
    )
    .toBe("# Target");
  await expect
    .poll(async () => {
      const caret = await page.locator(".cm-cursor-primary").boundingBox();
      return caret ? Math.abs(caret.y + caret.height / 2 - 400) : 100;
    })
    .toBeLessThan(3);
});

test("modifier-Enter follows the link under the cursor without inserting a line", async ({
  page,
}) => {
  await page.goto("/?fixture=links");
  const editor = page.getByRole("textbox", { name: "Markdown note" });
  await editor.locator(".cm-note-link").filter({ hasText: "[other]" }).click();
  const before = (await editor.textContent()) ?? "";
  await page.keyboard.press("ControlOrMeta+Enter");
  await expect(page.locator("body")).toHaveAttribute("data-link-action", "note");
  await expect(editor).toHaveText(before);
});

test("web autolinks use the same explicit modifier gesture", async ({ page }) => {
  await page.goto("/?fixture=links");
  await page
    .locator(".cm-note-link")
    .filter({ hasText: "<https://example.com>" })
    .click({ modifiers: ["ControlOrMeta"] });
  await expect(page.locator("body")).toHaveAttribute("data-link-action", "web");
});

test("code remains text and newly edited Markdown receives link affordances", async ({ page }) => {
  await page.goto("/?fixture=links");
  const editor = page.getByRole("textbox", { name: "Markdown note" });
  await expect(editor.locator(".cm-note-link")).toHaveCount(3);
  await editor.fill("[fresh](#target)\n\n`[code](#target)`\n\n# Target");
  await expect(editor.locator(".cm-note-link")).toHaveCount(1);
  await expect(editor.locator(".cm-note-link")).toHaveText("[fresh](#target)");
});

test("a missing link keeps the source editable and shows a dismissible error", async ({ page }) => {
  await page.goto("/?fixture=links&fault=missing-link");
  const editor = page.getByRole("textbox", { name: "Markdown note" });
  const before = (await editor.textContent()) ?? "";
  await editor
    .locator(".cm-note-link")
    .filter({ hasText: "[other]" })
    .click({ modifiers: ["ControlOrMeta"] });
  await expect(page.getByRole("alert")).toContainText("The linked file could not be found.");
  await expect(editor).toHaveText(before);
  await page.getByRole("button", { name: "Dismiss" }).click();
  await expect(page.getByRole("alert")).toBeHidden();
  await editor.fill("Still writing.");
  await expect(editor).toHaveText("Still writing.");
});

test("a linked window starts at its heading after bootstrap", async ({ page }) => {
  await page.goto("/?fixture=linked-heading");
  await expect
    .poll(() =>
      page.evaluate(
        () => window.getSelection()?.anchorNode?.parentElement?.closest(".cm-line")?.textContent,
      ),
    )
    .toBe("# Target");
});
