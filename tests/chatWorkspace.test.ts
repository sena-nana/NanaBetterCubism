import { fireEvent, render, screen } from "@testing-library/vue";
import { createMemoryHistory, createRouter } from "vue-router";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent, ref } from "vue";
import ChatPage from "../src/features/agent/ChatPage.vue";
import ConversationComposer from "../src/features/agent/components/ConversationComposer.vue";
import {
  clearConversationTurnPhase,
  getConversationRuntime,
} from "../src/features/agent/conversationRuntimeStore";
import type {
  AgentComputerOperationEvent,
  AgentPlanEvent,
  AgentToolEvent,
  AgentTurnDelta,
  AgentTurnFinished,
  AgentUserActionEvent,
  ChatMessage,
} from "../src/features/agent/types";

const listeners = vi.hoisted(() => ({
  computerOperation: null as null | ((payload: AgentComputerOperationEvent) => void),
  delta: null as null | ((payload: AgentTurnDelta) => void),
  plan: null as null | ((payload: AgentPlanEvent) => void),
  tool: null as null | ((payload: AgentToolEvent) => void),
  turnFinished: null as null | ((payload: AgentTurnFinished) => void),
  userAction: null as null | ((payload: AgentUserActionEvent) => void),
}));

const bridge = vi.hoisted(() => ({
  answerQuestion: vi.fn(async () => undefined),
  deleteConversation: vi.fn(async () => undefined),
  cancelTurn: vi.fn(async () => ({ state: "idle" as const })),
  decideComputerOperation: vi.fn(async () => undefined),
  getLlmConfig: vi.fn(async () => ({ baseUrl: null, model: "test-model", hasApiKey: true })),
  getMessages: vi.fn(),
  getPendingUserAction: vi.fn(async () => null),
  getPlan: vi.fn(async () => null),
  listConversations: vi.fn(async () => [
    { id: "a", title: "会话 A", projectId: null, projectName: null, updatedAt: "", pinned: false },
    { id: "b", title: "会话 B", projectId: null, projectName: null, updatedAt: "", pinned: false },
  ]),
  listProjects: vi.fn(async () => []),
  listenComputerOperation: vi.fn(async (handler) => {
    listeners.computerOperation = handler;
    return () => undefined;
  }),
  listenPlan: vi.fn(async (handler) => {
    listeners.plan = handler;
    return () => undefined;
  }),
  listenToolEvent: vi.fn(async (handler) => {
    listeners.tool = handler;
    return () => undefined;
  }),
  listenTurnDelta: vi.fn(async (handler) => {
    listeners.delta = handler;
    return () => undefined;
  }),
  listenTurnFinished: vi.fn(async (handler) => {
    listeners.turnFinished = handler;
    return () => undefined;
  }),
  listenUserAction: vi.fn(async (handler) => {
    listeners.userAction = handler;
    return () => undefined;
  }),
  normalizeCommandError: vi.fn((error: unknown) => ({ code: "test", message: String(error) })),
  sendMessage: vi.fn(async () => undefined),
  setConversationPinned: vi.fn(async () => true),
}));

vi.mock("../src/features/agent/bridge", () => bridge);

beforeEach(() => {
  clearConversationTurnPhase("a");
  clearConversationTurnPhase("b");
  bridge.answerQuestion.mockClear();
  bridge.cancelTurn.mockClear();
  bridge.decideComputerOperation.mockClear();
  bridge.getMessages.mockReset().mockResolvedValue([]);
  bridge.getPendingUserAction.mockReset().mockResolvedValue(null);
  bridge.getPlan.mockReset().mockResolvedValue(null);
  bridge.sendMessage.mockClear();
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((next) => {
    resolve = next;
  });
  return { promise, resolve };
}

function message(id: string, content: string): ChatMessage {
  return {
    id,
    role: "assistant",
    content,
    toolName: null,
    toolStatus: null,
    createdAt: "2026-07-15T00:00:00Z",
  };
}

describe("对话工作区", () => {
  it("不再渲染顶部标题和输入框内的连接状态", async () => {
    await renderChat("a");

    expect(document.querySelector('[data-agent-id="agent.chat.header"]')).toBeNull();
    expect(document.querySelector('[data-agent-id="agent.chat.open-model-settings"]')).toBeNull();
    expect(document.querySelector('[data-agent-id="agent.chat.open-editor-settings"]')).toBeNull();
  });

  it("composer 使用 Enter 发送、Shift+Enter 保留换行，并在提问时切换交互", async () => {
    const Host = defineComponent({
      components: { ConversationComposer },
      setup() {
        return { draft: ref("执行操作"), answer: ref("") };
      },
      template: `
        <ConversationComposer
          v-model="draft"
          v-model:ask-answer="answer"
          :can-send="true"
          :pending-action="{
            kind: 'question',
            actionId: 'ask-1',
            conversationId: 'a',
            question: '选择处理方式',
            options: ['继续']
          }"
          @send="$emit('sent')"
          @answer="(value) => $emit('answered', value)"
        />
      `,
    });
    const view = render(Host);

    const answerInput = screen.getByPlaceholderText("输入回答");
    await fireEvent.keyDown(answerInput, { key: "Enter", shiftKey: true });
    expect(view.emitted().answered).toBeUndefined();
    await fireEvent.click(screen.getByRole("button", { name: "继续" }));
    expect(view.emitted().answered?.[0]).toEqual(["继续"]);

    const regular = render(ConversationComposer, {
      props: { modelValue: "执行操作", canSend: true },
    });
    const input = screen.getByPlaceholderText("描述你想在 Cubism Editor 中完成的事…");
    await fireEvent.keyDown(input, { key: "Enter", shiftKey: true });
    expect(regular.emitted().send).toBeUndefined();
    await fireEvent.keyDown(input, { key: "Enter" });
    expect(regular.emitted().send).toHaveLength(1);
  });

  it("电脑代理授权只能通过独立的批准或拒绝操作", async () => {
    const view = render(ConversationComposer, {
      props: {
        modelValue: "",
        pendingAction: {
          kind: "computer_approval",
          actionId: "approval-1",
          conversationId: "a",
          goal: "调整 Warp 控制点",
          reason: "Cubism 没有可用于此操作的 API，只能由 Agent 代理操作 Cubism 窗口。",
          targetWindowTitle: "Cubism Editor",
          steps: [{ id: "move", title: "拖动控制点" }],
          allowedActions: ["drag"],
          includesFileDialogs: false,
          impact: "Agent 将向 Cubism 注入鼠标输入。",
          cannotUndo: true,
          expiresAt: "2026-07-15T00:05:00Z",
        },
      },
    });

    expect(screen.queryByPlaceholderText("输入回答")).toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "拒绝" }));
    await fireEvent.click(screen.getByRole("button", { name: "授权本次操作" }));
    expect(view.emitted().decide).toEqual([[false], [true]]);
  });

  it("授权事件绑定当前会话并调用独立授权命令", async () => {
    bridge.getMessages.mockResolvedValue([]);
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [{ path: "/chats/:id", component: ChatPage }],
    });
    await router.push("/chats/a");
    await router.isReady();
    render({ template: "<RouterView />" }, { global: { plugins: [router] } });
    await vi.waitFor(() => expect(listeners.userAction).not.toBeNull());

    const approval = {
      kind: "computer_approval" as const,
      actionId: "approval-a",
      conversationId: "a",
      goal: "调整 Warp 控制点",
      reason: "Cubism 没有可用于此操作的 API，只能由 Agent 代理操作 Cubism 窗口。",
      targetWindowTitle: "Cubism Editor",
      steps: [{ id: "move", title: "拖动控制点" }],
      allowedActions: ["drag" as const],
      includesFileDialogs: false,
      impact: "Agent 将向 Cubism 注入鼠标输入。",
      cannotUndo: true,
      expiresAt: "2026-07-15T00:05:00Z",
    };
    listeners.userAction?.({
      conversationId: "b",
      action: { ...approval, actionId: "approval-b", conversationId: "b" },
    });
    expect(screen.queryByRole("button", { name: "授权本次操作" })).toBeNull();

    await router.push("/chats/b");
    expect(await screen.findByRole("button", { name: "授权本次操作" })).toBeTruthy();
    await router.push("/chats/a");

    listeners.userAction?.({ conversationId: "a", action: approval });
    await fireEvent.click(await screen.findByRole("button", { name: "授权本次操作" }));
    expect(bridge.decideComputerOperation).toHaveBeenCalledWith("approval-a", true);
  });

  it("快速切换会话时忽略旧会话迟到的加载结果", async () => {
    const a = deferred<ChatMessage[]>();
    bridge.getMessages.mockImplementation((conversationId: string) =>
      conversationId === "a" ? a.promise : Promise.resolve([message("b-message", "B 的内容")]),
    );

    const router = createRouter({
      history: createMemoryHistory(),
      routes: [{ path: "/chats/:id", component: ChatPage }],
    });
    await router.push("/chats/a");
    await router.isReady();
    render({ template: "<RouterView />" }, { global: { plugins: [router] } });

    await vi.waitFor(() => expect(bridge.getMessages).toHaveBeenCalledWith("a"));
    await router.push("/chats/b");
    expect(await screen.findByText("B 的内容")).toBeTruthy();

    a.resolve([message("a-message", "A 的迟到内容")]);
    await Promise.resolve();
    await Promise.resolve();
    expect(screen.queryByText("A 的迟到内容")).toBeNull();
    expect(screen.getByText("B 的内容")).toBeTruthy();
  });

  it("切换后保持原对话运行，并允许另一对话同时发送", async () => {
    const router = await renderChat("a");
    const input = await screen.findByPlaceholderText("描述你想在 Cubism Editor 中完成的事…");
    await fireEvent.update(input, "A 请求");
    await fireEvent.keyDown(input, { key: "Enter" });
    expect(bridge.sendMessage).toHaveBeenCalledWith("a", "A 请求");

    await router.push("/chats/b");
    await waitForConversationLoad("b");
    const inputB = screen.getByPlaceholderText("描述你想在 Cubism Editor 中完成的事…");
    await fireEvent.update(inputB, "B 请求");
    await fireEvent.keyDown(inputB, { key: "Enter" });

    expect(bridge.sendMessage).toHaveBeenNthCalledWith(2, "b", "B 请求");
    expect(bridge.cancelTurn).not.toHaveBeenCalled();
    expect(getConversationRuntime("a").phase).toBe("running");
    expect(getConversationRuntime("b").phase).toBe("running");
    expect(getConversationRuntime("a").messages.map((item) => item.content)).toEqual(["A 请求"]);
    expect(getConversationRuntime("b").messages.map((item) => item.content)).toEqual(["B 请求"]);

    listeners.delta?.({ conversationId: "a", text: "A 后台输出" });
    listeners.delta?.({ conversationId: "b", text: "B 前台输出" });
    expect(await screen.findByText("B 前台输出")).toBeTruthy();
    expect(screen.queryByText("A 后台输出")).toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "停止" }));
    expect(bridge.cancelTurn).toHaveBeenCalledWith("b");
    expect(getConversationRuntime("a").phase).toBe("running");

    await router.push("/chats/a");
    expect(await screen.findByText("A 后台输出")).toBeTruthy();
  });

  it("分别保留各对话草稿，并在后台完成后以持久化消息收口", async () => {
    const router = await renderChat("a");
    const inputA = await screen.findByPlaceholderText("描述你想在 Cubism Editor 中完成的事…");
    await fireEvent.update(inputA, "A 草稿");

    await router.push("/chats/b");
    await waitForConversationLoad("b");
    const inputB = screen.getByPlaceholderText("描述你想在 Cubism Editor 中完成的事…");
    await fireEvent.update(inputB, "B 草稿");

    await router.push("/chats/a");
    expect(screen.getByPlaceholderText("描述你想在 Cubism Editor 中完成的事…")).toHaveValue(
      "A 草稿",
    );

    await fireEvent.keyDown(
      screen.getByPlaceholderText("描述你想在 Cubism Editor 中完成的事…"),
      { key: "Enter" },
    );
    await router.push("/chats/b");
    listeners.delta?.({ conversationId: "a", text: "A 实时片段" });
    bridge.getMessages.mockImplementation(async (conversationId: string) =>
      conversationId === "a" ? [message("a-final", "A 完整结果")] : [],
    );
    listeners.turnFinished?.({ conversationId: "a", ok: true, message: "完成" });
    await vi.waitFor(() => expect(getConversationRuntime("a").loading).toBe(false));

    expect(screen.getByPlaceholderText("描述你想在 Cubism Editor 中完成的事…")).toHaveValue(
      "B 草稿",
    );
    await router.push("/chats/a");
    expect(await screen.findByText("A 完整结果")).toBeTruthy();
    expect(screen.queryByText("A 实时片段")).toBeNull();
    expect(getConversationRuntime("a").phase).toBe("idle");
  });

  it("使用同一计划面板呈现重载状态与实时更新", async () => {
    bridge.getMessages.mockResolvedValue([]);
    bridge.getPlan.mockResolvedValue({
      conversationId: "a",
      steps: [{ id: "loaded", title: "已加载步骤", status: "pending" }],
      updatedAt: "2026-07-15T00:00:00Z",
    });

    const router = createRouter({
      history: createMemoryHistory(),
      routes: [{ path: "/chats/:id", component: ChatPage }],
    });
    await router.push("/chats/a");
    await router.isReady();
    render({ template: "<RouterView />" }, { global: { plugins: [router] } });

    expect(await screen.findByText("已加载步骤")).toBeTruthy();
    listeners.plan?.({
      conversationId: "a",
      plan: {
        conversationId: "a",
        steps: [{ id: "live", title: "实时更新步骤", status: "in_progress" }],
        updatedAt: "2026-07-15T00:01:00Z",
      },
    });
    expect(await screen.findByText("实时更新步骤")).toBeTruthy();
    expect(screen.queryByText("已加载步骤")).toBeNull();
  });
});

async function renderChat(conversationId: string) {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: "/chats/:id", component: ChatPage }],
  });
  await router.push(`/chats/${conversationId}`);
  await router.isReady();
  render({ template: "<RouterView />" }, { global: { plugins: [router] } });
  await vi.waitFor(() => expect(bridge.getMessages).toHaveBeenCalledWith(conversationId));
  return router;
}

async function waitForConversationLoad(conversationId: string) {
  await vi.waitFor(() => expect(bridge.getMessages).toHaveBeenCalledWith(conversationId));
  await vi.waitFor(() => expect(getConversationRuntime(conversationId).loading).toBe(false));
}
