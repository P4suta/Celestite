import { expect, test } from "./fixtures";

for (const fault of ["disk-full", "future-response"]) {
  test(`failed persistence preserves editable text and can be retried: ${fault}`, async ({
    page,
  }) => {
    await page.goto(`/?fixture=markdown&fault=${fault}`);
    const editor = page.getByRole("textbox", { name: "Markdown note" });
    const text = "# Keep every byte\n日本語 🦀 é\n";
    await editor.fill(text);
    await page.keyboard.press("ControlOrMeta+s");
    await expect(page.getByRole("alert")).toContainText("Couldn’t save");
    await expect(editor).toContainText("日本語 🦀 é");
    await page.keyboard.press("ControlOrMeta+w");
    await expect(page.getByRole("dialog")).toBeVisible();
    await page.getByRole("button", { name: "Cancel", exact: true }).click();
    await expect(editor).toBeFocused();
    await page.getByRole("button", { name: "Try Again", exact: true }).click();
    await expect(page.getByRole("alert")).toBeHidden();
    await expect(editor).toContainText("Keep every byte");
    await expect(page.locator("body")).not.toHaveAttribute("data-closed", "true");
  });
}

test("a conflict keeps local edits until Save As creates a separate copy", async ({ page }) => {
  await page.goto("/?fixture=markdown&fault=conflict");
  const editor = page.getByRole("textbox", { name: "Markdown note" });
  await editor.fill("Local draft 日本語 🦀");
  await page.keyboard.press("ControlOrMeta+s");
  await expect(page.getByRole("alert")).toContainText("File changed outside Celestite");
  await expect(editor).toContainText("Local draft 日本語 🦀");
  await page.getByRole("alert").getByRole("button", { name: "Save As…", exact: true }).click();
  await expect(page.getByRole("alert")).toBeHidden();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("button", { name: "File menu" })).toContainText("Recovered copy.md");
  await expect(editor).toContainText("Local draft 日本語 🦀");
});

test("cancelled Save As keeps the close dialog and draft intact", async ({ page }) => {
  await page.goto("/?fault=cancel-save");
  const editor = page.getByRole("textbox", { name: "Markdown note" });
  await editor.fill("Keep this unsaved draft.");
  await page.keyboard.press("ControlOrMeta+w");
  const dialog = page.getByRole("dialog");
  await dialog.getByRole("button", { name: "Save", exact: true }).click();
  await expect(dialog).toBeVisible();
  await expect(page.locator("body")).not.toHaveAttribute("data-closed", "true");
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(editor).toBeFocused();
  await expect(editor).toContainText("Keep this unsaved draft.");
});

test("malformed startup data produces a visible error and a read-only editor", async ({ page }) => {
  await page.goto("/?fault=invalid-bootstrap");
  await expect(page.getByRole("alert")).toBeVisible();
  const editor = page.getByRole("textbox", { name: "Markdown note" });
  await expect(editor).toHaveAttribute("aria-readonly", "true");
  await editor.click();
  await page.keyboard.type("must not enter the note");
  await expect(editor).toHaveText("");
});

test("revision exhaustion keeps the text visibly unsaved and prevents implicit closing", async ({
  page,
}) => {
  await page.goto("/?fixture=markdown&fault=revision-limit");
  const editor = page.getByRole("textbox", { name: "Markdown note" });
  await editor.fill("Keep the edit at the revision limit.");
  await expect(page.getByRole("alert")).toContainText("revision limit");
  await page.keyboard.press("Escape");
  await expect(page.getByRole("button", { name: "File menu" }).locator("span").first()).toHaveCSS(
    "font-weight",
    "700",
  );
  await page.keyboard.press("ControlOrMeta+w");
  await expect(page.getByRole("dialog")).toBeVisible();
  await page.getByRole("dialog").getByRole("button", { name: "Save", exact: true }).click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await expect(page.locator("body")).not.toHaveAttribute("data-closed", "true");
  await page.getByRole("dialog").getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(editor).toContainText("Keep the edit at the revision limit.");
});

for (const appearance of ["Light", "Dark"]) {
  test(`computed appearance preserves monochrome colors, menu rules, and no decoration: ${appearance}`, async ({
    page,
  }) => {
    await page.goto("/?fixture=markdown");
    await page.getByRole("button", { name: "File menu" }).click();
    await page.getByRole("menuitemradio", { name: appearance }).click();
    const paper = appearance === "Light" ? "rgb(255, 255, 255)" : "rgb(0, 0, 0)";
    const ink = appearance === "Light" ? "rgb(0, 0, 0)" : "rgb(255, 255, 255)";
    await expect(page.locator(".cm-editor")).toHaveCSS("background-color", paper);
    await expect(page.locator(".cm-editor")).toHaveCSS("color", ink);
    await expect(page.getByRole("separator")).toHaveCount(3);
    for (const rule of await page.getByRole("separator").all()) {
      await expect(rule).toHaveCSS("border-top-width", "1px");
      await expect(rule).toHaveCSS("border-top-color", ink);
    }
    const decorations = await page
      .locator("main, header, button, .file-menu, .cm-editor")
      .evaluateAll((elements) =>
        elements.map((element) => {
          const style = getComputedStyle(element);
          return { shadow: style.boxShadow, backgroundImage: style.backgroundImage };
        }),
      );
    expect(
      decorations.every((style) => style.shadow === "none" && style.backgroundImage === "none"),
    ).toBe(true);
  });
}
