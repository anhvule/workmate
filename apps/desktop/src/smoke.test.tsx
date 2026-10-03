// @vitest-environment jsdom
/**
 * A thin smoke pass over the real screens, driven through the mock bridge.
 * It proves the shell hangs together and the rules that carry weight hold in
 * what a user sees — not that every component renders (ticket 029).
 */
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const open = async (query: string) => {
  vi.resetModules();
  window.history.pushState({}, "", `/${query}`);
  const { App } = await import("./App.js");
  const { resetLive } = await import("./lib/live.js");
  resetLive();
  render(<App />);
  return userEvent.setup();
};

beforeEach(() => {
  // jsdom has no layout engine; Radix and the editor rely on these.
  window.HTMLElement.prototype.scrollIntoView = vi.fn();
  window.HTMLElement.prototype.hasPointerCapture = vi.fn();
  window.HTMLElement.prototype.releasePointerCapture = vi.fn();
  window.ResizeObserver = class { observe() {} unobserve() {} disconnect() {} } as unknown as typeof ResizeObserver;
});
afterEach(cleanup);

describe("first run", () => {
  it("asks only for a folder, then a choice, and lands in a project ready for a first message", async () => {
    const user = await open("?tick=1");
    await user.click(await screen.findByRole("button", { name: /choose a folder/i }));
    expect(await screen.findByText(/how do you want to work/i)).toBeTruthy();
    // The gentle option is the default; the team is one click away.
    expect(screen.getByRole("button", { name: /one assistant/i }).getAttribute("aria-pressed")).toBe("true");
    await user.click(screen.getByRole("button", { name: /open project/i }));

    expect(await screen.findByRole("tab", { name: "Runs" })).toBeTruthy();
    // No key and no model yet: it says so at the moment it matters, not before.
    expect(await screen.findByText(/choose a model/i)).toBeTruthy();
    expect((screen.getByRole("button", { name: "Start" }) as HTMLButtonElement).disabled).toBe(true);
  });
});

describe("a finished team run", () => {
  it("reads as an attributed thread with the handoffs made visible", async () => {
    const user = await open("?demo");
    await user.click(await screen.findByText("Add rate limiting to the public API"));
    const strip = await screen.findByLabelText("Roles");
    for (const r of ["Planner", "Builder", "Reviewer"]) expect(within(strip).getByText(r)).toBeTruthy();

    const cards = await screen.findAllByText(/^Handed off$/);
    expect(cards).toHaveLength(2);
    // Collapsed by default; one click shows exactly what was passed.
    expect(screen.queryByText(/What the previous role concluded/)).toBeNull();
    await user.click(screen.getAllByText(/see what was passed/)[0]!);
    expect(await screen.findByText(/What the previous role concluded/)).toBeTruthy();
  });
});

describe("a one-role run", () => {
  it("is a plain chat: no role strip, no handoff cards, and you can keep talking", async () => {
    const user = await open("?demo&tick=1");
    await user.click(await screen.findByRole("tab", { name: "Library" }));
    await user.click((await screen.findAllByRole("button", { name: /use…/i }))[0]!); // Solo builder
    await user.click(await screen.findByRole("button", { name: /create team/i }));
    await user.click(await screen.findByRole("tab", { name: "Runs" }));

    await user.selectOptions(await screen.findByLabelText("Team"), (await screen.findByRole("option", { name: /Solo/ })) as HTMLOptionElement);
    await user.type(screen.getByLabelText("What should we work on?"), "Rename the helper");
    await user.click(screen.getByRole("button", { name: "Start" }));

    await screen.findByRole("heading", { name: "Rename the helper" });
    // The box is there during the turn but disabled; it opens when the turn ends.
    await waitFor(() => expect((screen.getByLabelText("Message") as HTMLTextAreaElement).disabled).toBe(false), { timeout: 4000 });
    expect(screen.queryByLabelText("Roles")).toBeNull();
    expect(screen.queryByText(/handed off/i)).toBeNull();

    await user.type(screen.getByLabelText("Message"), "now add a test");
    await user.click(screen.getByRole("button", { name: "Send" }));
    expect(await screen.findByText("now add a test")).toBeTruthy();
  });
});

describe("intervening in a team run", () => {
  it("pauses at the handoff, shows exactly what the next role will be told, and lets you amend it", async () => {
    const user = await open("?demo&tick=1");
    await user.type(await screen.findByLabelText("What should we work on?"), "Add a health endpoint");
    await user.click(screen.getByLabelText(/pause at each handoff/i));
    await user.click(screen.getByRole("button", { name: "Start" }));

    const box = (await screen.findByLabelText("Handoff context", {}, { timeout: 4000 })) as HTMLTextAreaElement;
    expect(box.value).toContain("Add a health endpoint");
    expect(screen.getByRole("button", { name: /continue to builder/i })).toBeTruthy();

    await user.clear(box);
    await user.type(box, "Only touch the router.");
    await user.click(screen.getByRole("button", { name: /send amended to builder/i }));
    await waitFor(() => expect(screen.getAllByText(/handed off/i).length).toBeGreaterThan(0), { timeout: 4000 });
  });
});

describe("memory", () => {
  it("lists what is remembered with its scope and lets you forget it", async () => {
    const user = await open("?demo");
    await user.click(await screen.findByRole("tab", { name: "Memory" }));
    expect(await screen.findByText(/This project uses pnpm/)).toBeTruthy();
    expect(screen.getByText(/never recalled/)).toBeTruthy();

    await user.click(screen.getAllByRole("button", { name: "Forget" })[1]!);
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/older versions it replaced go with it/)).toBeTruthy();
    await user.click(within(dialog).getByRole("button", { name: "Forget" }));
    await waitFor(() => expect(screen.queryByText(/Conventional commits, imperative/)).toBeNull());
    expect(screen.getByText(/This project uses pnpm/)).toBeTruthy();
  });
});
