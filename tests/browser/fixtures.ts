import { test as base, expect } from "@playwright/test";

interface BrowserChecks {
  readonly uncaughtErrors: undefined;
}

export const test = base.extend<BrowserChecks>({
  uncaughtErrors: [
    async ({ page }, use): Promise<void> => {
      const errors: string[] = [];
      page.on("pageerror", (error) => {
        errors.push(error.message);
      });
      await use(undefined);
      expect(errors, "The application emitted an uncaught browser error.").toEqual([]);
    },
    { auto: true },
  ],
});

export { expect };
