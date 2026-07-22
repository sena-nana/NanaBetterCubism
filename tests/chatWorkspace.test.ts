import { fireEvent, render, screen } from "@testing-library/vue";
import { createMemoryHistory, createRouter } from "vue-router";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent, ref } from "vue";
import ChatPage from "../src/features/agent/ChatPage.vue";
import ConversationComposer from "../src/features/agent/components/ConversationComposer.vue";
import { useLlmConfigStore } from "../src/features/agent/llmConfigStore";
import HomePage from "../src/features/home/HomePage.vue";
import {
  clearConversationTurnPhase,
  getConversationRuntime,
} from "../src/features/agent/conversationRuntimeStore";
import type {
  AgentAskDraftEvent,
  AgentComputerOperationEvent,
  AgentPlanEvent,
  AgentToolEvent,
  AgentTurnDelta,
  AgentTurnFinished,
  AgentUserActionEvent,
  ChatMessage,
} from "../src/features/agent/types";

const listeners = vi.hoisted(() => ({
  askDraft: null as null | ((payload: AgentAskDraftEvent) => void),
  computerOperation: null as null | ((payload: AgentComputerOperationEvent) => void),
  delta: null as null | ((payload: AgentTurnDelta) => void),
  plan: null as null | ((payload: AgentPlanEvent) => void),
  tool: null as null | ((payload: AgentToolEvent) => void),
  turnFinished: null as null | ((payload: AgentTurnFinished) => void),
  userAction: null as null | ((payload: AgentUserActionEvent) => void),
}));

const bridge = vi.hoisted(() => ({
  answerQuestion: vi.fn(async () => undefined),
  createConversation: vi.fn(async () => ({
    id: "new-conversation",
    title: "新对话",
    projectId: null,
    projectName: null,
    updatedAt: "",
    pinned: false,
  })),
  deleteConversation: vi.fn(async () => undefined),
  cancelTurn: vi.fn(async () => ({ state: "idle" as const })),
  decidePlan: vi.fn(async () => "execution_started" as const),
  getLlmConfig: vi.fn(async () => ({ baseUrl: null, model: "test-model", hasApiKey: true })),
  getMessages: vi.fn(),
  getPendingUserAction: vi.fn(async () => null),
  getPlan: vi.fn(async () => null),
  listConversations: vi.fn(async () => [
    { id: "a", title: "会话 A", projectId: null, projectName: null, updatedAt: "", pinned: false },
    { id: "b", title: "会话 B", projectId: null, projectName: null, updatedAt: "", pinned: false },
  ]),
  listProjects: vi.fn(async () => []),
  listPsds: vi.fn(async () => []),
  listenImageCapability: vi.fn(async () => () => {}),
  listenAskDraft: vi.fn(async (handler) => {
    listeners.askDraft = handler;
    return () => undefined;
  }),
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
  sendMessage: vi.fn(async (_conversationId: string, content: string) => ({
    id: `persisted-${content}`,
    role: "user" as const,
    content,
    toolName: null,
    toolDisplayName: null,
    toolStatus: null,
    attachments: [],
    createdAt: "2026-07-15T00:00:00Z",
  })),
  setConversationPinned: vi.fn(async () => true),
  testLlmConnection: vi.fn(),
}));

vi.mock("../src/features/agent/bridge", () => bridge);

const completeLlmConfig = {
  baseUrl: "https://api.example.test/v1",
  model: "test-model",
  hasApiKey: true,
};

const successfulLlmCheck = {
  ok: true,
  message: "connected",
  models: ["test-model"],
};

beforeEach(async () => {
  clearConversationTurnPhase("a");
  clearConversationTurnPhase("b");
  bridge.answerQuestion.mockClear();
  bridge.createConversation.mockClear();
  bridge.cancelTurn.mockClear();
  bridge.decidePlan.mockReset().mockResolvedValue("execution_started");
  bridge.getMessages.mockReset().mockResolvedValue([]);
  bridge.getPendingUserAction.mockReset().mockResolvedValue(null);
  bridge.getPlan.mockReset().mockResolvedValue(null);
  bridge.sendMessage.mockClear();
  bridge.testLlmConnection.mockReset().mockResolvedValue(successfulLlmCheck);
  const llm = useLlmConfigStore();
  llm.applyConfig(completeLlmConfig);
  await llm.testConnection();
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
    toolDisplayName: null,
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
        return {
          draft: ref("执行操作"),
          answer: ref(""),
          pendingAction: {
            kind: "question",
            actionId: "ask-1",
            conversationId: "a",
            question: "选择处理方式\n\n| 方式 | 影响 |\n| --- | --- |\n| 继续 | 保留当前进度 |",
            options: ["继续"],
          },
        };
      },
      template: `
        <ConversationComposer
          v-model="draft"
          v-model:ask-answer="answer"
          :can-send="true"
          :pending-action="pendingAction"
          @send="$emit('sent')"
          @answer="(value) => $emit('answered', value)"
        />
      `,
    });
    const view = render(Host);

    expect(view.container.querySelector("[data-agent-id='agent.chat.ask'] table")).toBeTruthy();
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

  it("在 Ask 卡片中流式呈现计划和正文，完成后才开放回答", async () => {
    await renderChat("a");
    const input = await screen.findByPlaceholderText("描述你想在 Cubism Editor 中完成的事…");
    await fireEvent.update(input, "调整参数");
    await fireEvent.keyDown(input, { key: "Enter" });

    listeners.delta?.({ conversationId: "a", text: "我会先核对当前状态。" });
    listeners.askDraft?.({
      conversationId: "a",
      question: "## 操作计划\n\n1. 核对参数",
    });

    expect(await screen.findByText("我会先核对当前状态。")).toBeTruthy();
    const streamingAsk = document.querySelector('[data-agent-id="agent.chat.ask"]');
    expect(streamingAsk).toBeTruthy();
    expect(streamingAsk?.querySelector("ol")).toBeTruthy();
    expect(document.querySelector('[data-agent-id="agent.chat.ask-streaming"]')).toBeTruthy();
    expect(screen.queryByPlaceholderText("输入回答")).toBeNull();

    listeners.askDraft?.({
      conversationId: "a",
      question: "## 操作计划\n\n1. 核对参数\n2. 执行调整\n\n| 操作 | 影响 |\n| --- | --- |\n| 调整 | 当前参数 |",
    });
    await vi.waitFor(() =>
      expect(document.querySelector('[data-agent-id="agent.chat.ask"] table')).toBeTruthy()
    );
    expect(getConversationRuntime("a").askDraft).toContain("执行调整");

    listeners.userAction?.({
      conversationId: "a",
      action: {
        kind: "question",
        actionId: "ask-stream",
        conversationId: "a",
        question: "确认执行上述计划？",
        options: ["继续"],
      },
    });

    expect(getConversationRuntime("a").askDraft).toBeNull();
    expect(await screen.findByPlaceholderText("输入回答")).toBeTruthy();
    expect(screen.getByRole("button", { name: "继续" })).toBeTruthy();
    expect(document.querySelector('[data-agent-id="agent.chat.ask-streaming"]')).toBeNull();

    listeners.askDraft?.({ conversationId: "a", question: "迟到的草稿" });
    expect(getConversationRuntime("a").askDraft).toBeNull();
    expect(screen.getByPlaceholderText("输入回答")).toBeTruthy();
  });

  it.each([
    ["会话页", () => renderChat("a"), "a"],
    ["首页", renderHome, "new-conversation"],
  ] as const)("%s composer 仅在模型就绪时发送并保留草稿", async (_, renderHost, conversationId) => {
    await renderHost();
    const llm = useLlmConfigStore();
    const input = await screen.findByPlaceholderText("描述你想在 Cubism Editor 中完成的事…");
    const send = screen.getByRole("button", { name: "发送" });
    await fireEvent.update(input, "保留这份草稿");
    expect(send).toBeEnabled();

    bridge.testLlmConnection.mockResolvedValueOnce({ ok: false, message: "unavailable", models: [] });
    await llm.testConnection();
    await vi.waitFor(() => expect(send).toBeDisabled());
    expect(input).toHaveValue("保留这份草稿");
    await fireEvent.keyDown(input, { key: "Enter" });
    expect(bridge.sendMessage).not.toHaveBeenCalled();

    bridge.testLlmConnection.mockResolvedValueOnce(successfulLlmCheck);
    await llm.testConnection();
    expect(send).toBeEnabled();

    llm.applyConfig({ ...completeLlmConfig, model: "next-model" });
    await vi.waitFor(() => expect(send).toBeDisabled());
    expect(input).toHaveValue("保留这份草稿");
    await fireEvent.click(send);
    expect(bridge.sendMessage).not.toHaveBeenCalled();

    bridge.testLlmConnection.mockResolvedValueOnce(successfulLlmCheck);
    await llm.testConnection();
    await fireEvent.click(send);
    await vi.waitFor(() => expect(bridge.sendMessage).toHaveBeenCalledWith(
      conversationId,
      "保留这份草稿",
      [],
      "default",
    ));
  });

  it("使用现有提问面板处理保存到项目记忆的选择", async () => {
    const view = render(ConversationComposer, {
      props: {
        modelValue: "",
        pendingAction: {
          kind: "question",
          actionId: "memory-offer",
          conversationId: "a",
          question: "是否将这次已验证的修改保存到项目记忆？",
          options: ["保存到项目记忆", "暂不保存"],
        },
      },
    });

    expect(screen.queryByRole("button", { name: "授权本次操作" })).toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "保存到项目记忆" }));
    expect(view.emitted().answer).toEqual([["保存到项目记忆"]]);
  });

  it("计划确认支持修改、取消和执行，Enter 只提交非空修改", async () => {
    const view = render(ConversationComposer, {
      props: {
        modelValue: "",
        planRevision: "补充参数回读验收",
        pendingAction: {
          kind: "plan_approval",
          actionId: "plan-1",
          conversationId: "a",
          title: "整理参数结构",
        },
      },
    });

    const revision = screen.getByPlaceholderText("输入修改要求");
    await fireEvent.keyDown(revision, { key: "Enter", shiftKey: true });
    expect(view.emitted().decidePlan).toBeUndefined();
    await fireEvent.keyDown(revision, { key: "Enter" });
    await fireEvent.click(screen.getByRole("button", { name: "取消" }));
    await fireEvent.click(screen.getByRole("button", { name: "按计划执行" }));
    expect(view.emitted().decidePlan).toEqual([
      ["revise", "补充参数回读验收"],
      ["cancel"],
      ["approve"],
    ]);
  });

  it("计划与权限下拉互斥，并按对话保留模式", async () => {
    const router = await renderChat("a");
    const planButton = await screen.findByRole("button", { name: "计划" });
    await fireEvent.click(planButton);
    expect(planButton.getAttribute("aria-pressed")).toBe("true");
    // 计划模式下权限下拉触发器显示基线"询问"
    expect(screen.getByRole("button", { name: "询问" })).toBeTruthy();

    await router.push("/chats/b");
    await waitForConversationLoad("b");
    expect(screen.getByRole("button", { name: "计划" }).getAttribute("aria-pressed")).toBe("false");
    await fireEvent.click(screen.getByRole("button", { name: "询问" }));
    await fireEvent.click(await screen.findByRole("menuitem", { name: "仅读取" }));
    expect(getConversationRuntime("b").composerMode).toBe("conversation_only");
    expect(screen.getByRole("button", { name: "仅读取" })).toBeTruthy();

    await router.push("/chats/a");
    expect((await screen.findByRole("button", { name: "计划" })).getAttribute("aria-pressed")).toBe("true");
  });

  it("自动批准模式以黄色字体发送 auto_approve 模式", async () => {
    await renderChat("a");
    const trigger = await screen.findByRole("button", { name: "询问" });
    await fireEvent.click(trigger);
    await fireEvent.click(await screen.findByRole("menuitem", { name: "自动批准" }));
    expect(getConversationRuntime("a").composerMode).toBe("auto_approve");
    const autoTrigger = screen.getByRole("button", { name: "自动批准" });
    expect(autoTrigger.className).toContain("conversation-composer__permission--auto");

    const input = screen.getByPlaceholderText("描述你想在 Cubism Editor 中完成的事…");
    await fireEvent.update(input, "自主整理参数");
    await fireEvent.keyDown(input, { key: "Enter" });
    await vi.waitFor(() =>
      expect(bridge.sendMessage).toHaveBeenCalledWith(
        "a",
        "自主整理参数",
        [],
        "auto_approve",
      ),
    );
  });

  it("计划决策失败时保留待确认状态，成功后按真实结果切换模式", async () => {
    bridge.getPendingUserAction.mockResolvedValue({
      kind: "plan_approval",
      actionId: "plan-a",
      conversationId: "a",
      title: "参数计划",
    });
    bridge.decidePlan.mockRejectedValueOnce(new Error("暂时失败"));
    await renderChat("a");

    await fireEvent.click(await screen.findByRole("button", { name: "按计划执行" }));
    expect(await screen.findByRole("button", { name: "按计划执行" })).toBeTruthy();
    expect(getConversationRuntime("a").phase).toBe("awaiting_input");

    await fireEvent.click(screen.getByRole("button", { name: "按计划执行" }));
    expect(bridge.decidePlan).toHaveBeenLastCalledWith("plan-a", "approve", undefined);
    expect(getConversationRuntime("a").composerMode).toBe("default");
    expect(getConversationRuntime("a").phase).toBe("running");
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
    expect(bridge.sendMessage).toHaveBeenCalledWith("a", "A 请求", [], "default");

    await router.push("/chats/b");
    await waitForConversationLoad("b");
    const inputB = screen.getByPlaceholderText("描述你想在 Cubism Editor 中完成的事…");
    await fireEvent.update(inputB, "B 请求");
    await fireEvent.keyDown(inputB, { key: "Enter" });

    expect(bridge.sendMessage).toHaveBeenNthCalledWith(2, "b", "B 请求", [], "default");
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

  it("按调用标识合并工具状态并展示后端可读名称", async () => {
    await renderChat("a");

    listeners.tool?.({
      conversationId: "a",
      toolCallId: "call-parameter-1",
      toolName: "get_parameter_values",
      toolDisplayName: "读取参数值",
      status: "started",
      summary: "",
    });
    expect(await screen.findByText("读取参数值")).toBeTruthy();
    expect(screen.getByText("进行中")).toBeTruthy();

    listeners.tool?.({
      conversationId: "a",
      toolCallId: "call-parameter-1",
      toolName: "get_parameter_values",
      toolDisplayName: "读取参数值",
      status: "finished",
      summary: "{}",
    });
    expect(await screen.findByText("完成")).toBeTruthy();
    expect(screen.getAllByText("读取参数值")).toHaveLength(1);

    listeners.tool?.({
      conversationId: "a",
      toolCallId: "call-parameter-2",
      toolName: "get_parameter_values",
      toolDisplayName: "读取参数值",
      status: "finished",
      summary: "{}",
    });
    listeners.tool?.({
      conversationId: "a",
      toolCallId: "call-skill",
      toolName: "read_skill",
      toolDisplayName: "读取任务技能",
      status: "finished",
      summary: "已读取任务技能",
    });
    expect(await screen.findByText("读取参数值 ×2、读取任务技能")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { expanded: false }));
    expect(screen.getAllByText("读取参数值")).toHaveLength(2);
    expect(screen.getByText("读取任务技能")).toBeTruthy();
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

async function renderHome() {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/", component: HomePage },
      { path: "/chats/:id", component: { template: "<div />" } },
    ],
  });
  await router.push("/");
  await router.isReady();
  render({ template: "<RouterView />" }, { global: { plugins: [router] } });
}

async function waitForConversationLoad(conversationId: string) {
  await vi.waitFor(() => expect(bridge.getMessages).toHaveBeenCalledWith(conversationId));
  await vi.waitFor(() => expect(getConversationRuntime(conversationId).loading).toBe(false));
}
