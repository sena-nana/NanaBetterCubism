import { afterEach, describe, expect, it, vi } from "vitest";
import type { App } from "vue";

let mountedApp: App | null = null;
let mountedRoot: HTMLElement | null = null;

afterEach(() => {
  mountedApp?.unmount();
  mountedRoot?.remove();
  mountedApp = null;
  mountedRoot = null;
  delete window.__LILIA_NATIVE_PLATFORM__;
});

describe("侧边栏表面模式", () => {
  it("让普通侧边栏跟随 Windows 透明区域设置", async () => {
    window.__LILIA_NATIVE_PLATFORM__ = "windows";
    vi.resetModules();

    const { nextTick } = await import("vue");
    const { createMemoryHistory } = await import("vue-router");
    const { createNanaBetterCubismApp } = await import("../src/app");
    const { useNativeAppearance } = await import("../src/ui");

    const { app, router } = createNanaBetterCubismApp(createMemoryHistory());
    await router.push("/");
    await router.isReady();

    const root = document.createElement("div");
    document.body.append(root);
    app.mount(root);
    mountedApp = app;
    mountedRoot = root;
    await nextTick();

    const navigation = document.querySelector('[data-agent-id="workspace.region.navigation"]');
    const sidebar = document.querySelector('[data-agent-id="sidebar.main"]');
    const primary = document.querySelector('[data-agent-id="workspace.region.primary"]');

    expect(document.documentElement.dataset.backdrop).toBe("mica");
    expect(navigation).toHaveAttribute("data-lilia-surface-mode", "translucent");
    expect(sidebar).toHaveAttribute("data-lilia-surface-mode", "translucent");
    expect(primary).toHaveAttribute("data-lilia-surface-mode", "solid");

    useNativeAppearance().setBackdropTarget("main");
    await nextTick();

    expect(navigation).toHaveAttribute("data-lilia-surface-mode", "solid");
    expect(sidebar).toHaveAttribute("data-lilia-surface-mode", "solid");
    expect(primary).toHaveAttribute("data-lilia-surface-mode", "translucent");
  });
});
