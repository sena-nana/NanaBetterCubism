import { fireEvent, render, screen } from "@testing-library/vue";
import { createMemoryHistory, createRouter } from "vue-router";
import { describe, expect, it, vi } from "vitest";
import { defineComponent, ref } from "vue";
import ChatPage from "../src/features/agent/ChatPage.vue";
import ConversationComposer from "../src/features/agent/components/ConversationComposer.vue";
import type { ChatMessage } from "../src/features/agent/types";

const bridge = vi.hoisted(() => ({
  answerAsk: vi.fn(async () => undefined),
  archiveConversation: vi.fn(async () => true),
  bindProject: vi.fn(async () => undefined),
  cancelTurn: vi.fn(async () => ({ state: "idle" as const })),
  consolidateMemory: vi.fn(async () => undefined),
  getLlmConfig: vi.fn(async () => ({ baseUrl: null, model: "test-model", hasApiKey: true })),
  getMessages: vi.fn(),
  getPendingAsk: vi.fn(async () => null),
  getPlan: vi.fn(async () => null),
  listConversations: vi.fn(async () => [
    { id: "a", title: "会话 A", projectId: null, projectName: null, updatedAt: "", pinned: false },
    { id: "b", title: "会话 B", projectId: null, projectName: null, updatedAt: "", pinned: false },
  ]),
  listProjects: vi.fn(async () => []),
  listenAsk: vi.fn(async () => () => undefined),
  listenPlan: vi.fn(async () => () => undefined),
  listenToolEvent: vi.fn(async () => () => undefined),
  listenTurnDelta: vi.fn(async () => () => undefined),
  listenTurnFinished: vi.fn(async () => () => undefined),
  normalizeCommandError: vi.fn((error: unknown) => ({ code: "test", message: String(error) })),
  sendMessage: vi.fn(async () => undefined),
  setConversationPinned: vi.fn(async () => true),
  upsertProject: vi.fn(),
}));

vi.mock("../src/features/agent/bridge", () => bridge);

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
          :pending-ask="{
            askId: 'ask-1',
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
});
