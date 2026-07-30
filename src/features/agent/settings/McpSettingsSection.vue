<script setup lang="ts">
import { Button, Card, Input } from "../../../ui";
import { computed, onMounted, onUnmounted, ref } from "vue";
import {
  getMcpStatus,
  listenMcpState,
  normalizeCommandError,
  rotateMcpToken,
  setMcpConfig,
  stopMcp,
} from "../bridge";
import type { McpStatus } from "../types";

type Operation = "save" | "start" | "stop" | "rotate";
type Feedback = { text: string; tone: "ok" | "err" };

const status = ref<McpStatus | null>(null);
const port = ref("3920");
const allowWrites = ref(true);
const showToken = ref(false);
const loading = ref(true);
const operation = ref<Operation | null>(null);
const feedback = ref<Feedback | null>(null);
const operationBusy = computed(
  () =>
    loading.value ||
    operation.value !== null ||
    status.value?.state === "starting" ||
    status.value?.state === "stopping",
);

let unlisten: (() => void) | null = null;

onMounted(async () => {
  try {
    applyStatus(await getMcpStatus());
    unlisten = await listenMcpState((next) => applyStatus(next));
  } catch (err) {
    setError(err);
  } finally {
    loading.value = false;
  }
});

onUnmounted(() => {
  unlisten?.();
  unlisten = null;
});

function applyStatus(next: McpStatus) {
  status.value = next;
  port.value = String(next.port);
  allowWrites.value = next.allowWrites;
}

function setError(error: unknown) {
  feedback.value = { text: normalizeCommandError(error).message, tone: "err" };
}

function parsePort(): number | null {
  const parsed = Number(port.value.trim());
  if (!Number.isInteger(parsed) || parsed < 1 || parsed > 65535) return null;
  return parsed;
}

const stateLabel = computed(() => {
  switch (status.value?.state) {
    case "running":
      return "运行中";
    case "starting":
      return "启动中";
    case "stopping":
      return "停止中";
    case "failed":
      return "失败";
    default:
      return "已停止";
  }
});

const cursorSnippet = computed(() => {
  if (!status.value?.url || !status.value.token) return "";
  return JSON.stringify(
    {
      mcpServers: {
        nanabettercubism: {
          url: status.value.url,
          headers: {
            Authorization: `Bearer ${status.value.token}`,
          },
        },
      },
    },
    null,
    2,
  );
});

async function save() {
  const nextPort = parsePort();
  if (nextPort == null) {
    feedback.value = { text: "端口必须是 1 到 65535 的整数。", tone: "err" };
    return;
  }
  operation.value = "save";
  feedback.value = null;
  try {
    applyStatus(
      await setMcpConfig({
        port: nextPort,
        allowWrites: allowWrites.value,
      }),
    );
    feedback.value = { text: "已保存 MCP 配置。", tone: "ok" };
  } catch (err) {
    setError(err);
  } finally {
    operation.value = null;
  }
}

async function start() {
  const nextPort = parsePort();
  if (nextPort == null) {
    feedback.value = { text: "端口必须是 1 到 65535 的整数。", tone: "err" };
    return;
  }
  operation.value = "start";
  feedback.value = null;
  try {
    applyStatus(
      await setMcpConfig({
        port: nextPort,
        allowWrites: allowWrites.value,
        enabled: true,
      }),
    );
    feedback.value = { text: "MCP 已启动。", tone: "ok" };
  } catch (err) {
    setError(err);
  } finally {
    operation.value = null;
  }
}

async function stop() {
  operation.value = "stop";
  feedback.value = null;
  try {
    applyStatus(await stopMcp());
    feedback.value = { text: "MCP 已停止。", tone: "ok" };
  } catch (err) {
    setError(err);
  } finally {
    operation.value = null;
  }
}

async function rotate() {
  operation.value = "rotate";
  feedback.value = null;
  try {
    applyStatus(await rotateMcpToken());
    showToken.value = true;
    feedback.value = { text: "已轮换 Bearer Token。", tone: "ok" };
  } catch (err) {
    setError(err);
  } finally {
    operation.value = null;
  }
}

async function copyText(text: string, success: string) {
  try {
    await navigator.clipboard.writeText(text);
    feedback.value = { text: success, tone: "ok" };
  } catch {
    feedback.value = { text: "复制失败，请手动选择文本。", tone: "err" };
  }
}
</script>

<template>
  <section class="settings-section" data-agent-id="settings.mcp">
    <Card :loading="loading" agent-id="settings.mcp.card">
      <h2>MCP Server</h2>
      <p class="card-description">
        在本机 127.0.0.1 暴露 Streamable HTTP MCP，供外部 Agent 中转操作 Cubism 与检查 PSD。默认关闭；开启后需 Bearer Token。
      </p>

      <template v-if="!loading && status">
        <label class="settings-field">
          <span><strong>状态</strong><small>{{ status.message }}</small></span>
          <span class="status-value" data-agent-id="settings.mcp.state">{{ stateLabel }}</span>
        </label>

        <label class="settings-field">
          <span><strong>端口</strong><small>仅监听 127.0.0.1；冲突时会报错，不会静默改绑。</small></span>
          <Input
            v-model="port"
            type="number"
            :disabled="operationBusy"
            agent-id="settings.mcp.port"
          />
        </label>

        <label class="settings-field">
          <span><strong>允许写入</strong><small>关闭后隐藏 Editor 写事务工具；连接与 PSD 检查仍可用。</small></span>
          <label class="toggle">
            <input
              v-model="allowWrites"
              type="checkbox"
              :disabled="operationBusy"
              data-agent-id="settings.mcp.allow-writes"
            />
            <span>{{ allowWrites ? "已开启" : "已关闭" }}</span>
          </label>
        </label>

        <label class="settings-field settings-field--stacked">
          <span><strong>Endpoint</strong><small>Cursor 等客户端使用的 Streamable HTTP URL。</small></span>
          <code class="mono" data-agent-id="settings.mcp.url">{{ status.url ?? `http://127.0.0.1:${port}/mcp` }}</code>
        </label>

        <label class="settings-field settings-field--stacked">
          <span>
            <strong>Bearer Token</strong>
            <small>保存在本机凭据库。外部 Agent 请求需带 Authorization 头。</small>
          </span>
          <div class="token-row">
            <code class="mono" data-agent-id="settings.mcp.token">
              {{ showToken ? status.token : "••••••••••••" }}
            </code>
            <Button
              :disabled="operationBusy || !status.token"
              agent-id="settings.mcp.toggle-token"
              @click="showToken = !showToken"
            >
              {{ showToken ? "隐藏" : "显示" }}
            </Button>
            <Button
              :disabled="operationBusy || !status.token"
              agent-id="settings.mcp.copy-token"
              @click="copyText(status.token, '已复制 Token。')"
            >
              复制
            </Button>
          </div>
        </label>

        <div v-if="cursorSnippet" class="settings-field settings-field--stacked">
          <span><strong>Cursor 配置片段</strong><small>写入 MCP 设置后即可连接。</small></span>
          <pre class="snippet" data-agent-id="settings.mcp.cursor-snippet">{{ cursorSnippet }}</pre>
          <Button
            :disabled="operationBusy"
            agent-id="settings.mcp.copy-snippet"
            @click="copyText(cursorSnippet, '已复制 Cursor 配置。')"
          >
            复制配置
          </Button>
        </div>

        <div class="actions">
          <Button
            variant="primary"
            :loading="operation === 'start'"
            :disabled="operationBusy || status.state === 'running' || status.state === 'starting'"
            agent-id="settings.mcp.start"
            @click="start"
          >
            启动
          </Button>
          <Button
            :loading="operation === 'stop'"
            :disabled="operationBusy || status.state === 'stopped'"
            agent-id="settings.mcp.stop"
            @click="stop"
          >
            停止
          </Button>
          <Button
            :loading="operation === 'save'"
            :disabled="operationBusy"
            agent-id="settings.mcp.save"
            @click="save"
          >
            保存配置
          </Button>
          <Button
            :loading="operation === 'rotate'"
            :disabled="operationBusy"
            agent-id="settings.mcp.rotate"
            @click="rotate"
          >
            轮换 Token
          </Button>
        </div>

        <p
          v-if="feedback"
          class="message"
          :class="`message--${feedback.tone}`"
          :role="feedback.tone === 'err' ? 'alert' : undefined"
          data-agent-id="settings.mcp.message"
        >
          {{ feedback.text }}
        </p>
      </template>
    </Card>
  </section>
</template>

<style scoped>
.card-description {
  margin: -2px 0 8px;
  color: var(--text-muted);
  font-size: 12px;
  line-height: 1.5;
}
.settings-field {
  display: grid;
  grid-template-columns: minmax(150px, .6fr) minmax(220px, 1fr);
  align-items: center;
  gap: 16px;
  padding: 10px 0;
}
.settings-field > span { display: grid; gap: 3px; }
.settings-field strong { font-size: 12px; }
.settings-field small { color: var(--text-muted); line-height: 1.4; }
.settings-field--stacked { grid-template-columns: 1fr; }
.status-value { font-size: 13px; font-weight: 600; }
.toggle { display: inline-flex; align-items: center; gap: 8px; font-size: 13px; }
.token-row { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; }
.mono {
  display: block;
  padding: 8px 10px;
  border: 1px solid var(--border);
  border-radius: 4px;
  font-size: 12px;
  word-break: break-all;
  background: transparent;
}
.snippet {
  margin: 0;
  padding: 10px;
  border: 1px solid var(--border);
  border-radius: 4px;
  font-size: 11px;
  white-space: pre-wrap;
  overflow: auto;
  max-height: 220px;
}
.actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-top: 16px;
}
.message {
  margin-top: 12px;
  font-size: 12px;
  color: var(--text-muted);
}
.message--ok { color: var(--ok); }
.message--err { color: var(--err); }
</style>
