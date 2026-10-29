import { expect, test } from "./fixtures";

const libraryId = "019a3f1e-7c2d-7b8a-9e21-0f4b6c8d1a2e";
const objectId = "019a3f25-9b0e-7d12-a3c7-5e1f8a2b6d90";

test("a library note shows its heading as the title once it is written", async ({ page }) => {
  await page.goto("/?fixture=library");
  const editor = page.getByRole("textbox", { name: "Markdown note" });
  await expect(editor).toContainText(`id: ${libraryId}`);
  await page.keyboard.press("Escape");
  const filename = page.getByRole("button", { name: "File menu" });
  await expect(filename).toContainText(`${libraryId}.md`);
  await editor.click();
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.insertText("# Object\n");
  await page.keyboard.press("Escape");
  await expect(filename).toContainText("Object");
  await expect(filename).not.toContainText(`${libraryId}.md`);
});

test("typing a bracket offers library titles and Enter writes the label and definition", async ({
  page,
}) => {
  await page.goto("/?fixture=library");
  const editor = page.getByRole("textbox", { name: "Markdown note" });
  await editor.click();
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.insertText("# Java\n\nSee ");
  await page.keyboard.type("[Obj");
  const list = page.locator(".note-completion");
  await expect(list).toBeVisible();
  await expect(list.getByRole("option")).toHaveCount(2);
  await expect(list.getByRole("option").first()).toContainText("Object");
  await expect(list).toHaveCSS("box-shadow", "none");
  const colors = await list.evaluate((element) => {
    const style = getComputedStyle(element);
    return [style.backgroundColor, style.color, style.borderTopColor];
  });
  expect(colors).toEqual(["rgb(255, 255, 255)", "rgb(0, 0, 0)", "rgb(0, 0, 0)"]);
  await page.keyboard.press("Enter");
  await expect(editor).toContainText("See [Obj]");
  await expect(editor).toContainText(`[Obj]: ${objectId}.md`);
  await expect(list).toBeHidden();
  await page.keyboard.press("ControlOrMeta+z");
  await expect(editor).not.toContainText(`[Obj]: ${objectId}.md`);
  await expect(editor).toContainText("See [Obj");
  await expect(editor).not.toContainText("See [Obj]");
  await page.keyboard.press("ControlOrMeta+Shift+z");
  await expect(editor).toContainText("See [Obj]");
  await expect(editor).toContainText(`[Obj]: ${objectId}.md`);
});

test("Escape dismisses the list without editing and code spans never complete", async ({
  page,
}) => {
  await page.goto("/?fixture=library");
  const editor = page.getByRole("textbox", { name: "Markdown note" });
  await editor.click();
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.type("See [Sub");
  const list = page.locator(".note-completion");
  await expect(list).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(list).toBeHidden();
  await expect(editor).toContainText("See [Sub");
  await expect(editor).not.toContainText("[Sub]:");
  await page.keyboard.type(" `[Ob`");
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.type("j");
  await page.waitForTimeout(300);
  await expect(list).toBeHidden();
  await expect(editor).toContainText("`[Obj`");
  await page.keyboard.press("End");
  await page.keyboard.type(" ![Obj");
  await expect(list).toBeVisible();
  await page.keyboard.press("Enter");
  await expect(editor).toContainText("![Obj]");
  await expect(editor).toContainText(`[Obj]: ${objectId}.md`);
});

test("a refused label keeps the words and shows a dismissible error", async ({ page }) => {
  await page.goto("/?fixture=library&fault=duplicate-label");
  const editor = page.getByRole("textbox", { name: "Markdown note" });
  await editor.click();
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.type("See [Obj");
  await expect(page.locator(".note-completion")).toBeVisible();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("alert")).toContainText("already links");
  await expect(editor).toContainText("See [Obj");
  await expect(editor).not.toContainText("[Obj]:");
  await page.getByRole("button", { name: "Dismiss" }).click();
  await expect(page.getByRole("alert")).toBeHidden();
});
