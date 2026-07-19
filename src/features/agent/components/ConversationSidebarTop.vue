<script setup lang="ts">
import MessageSquarePlus from "@lucide/vue/dist/esm/icons/message-square-plus.mjs";
import Search from "@lucide/vue/dist/esm/icons/search.mjs";
import X from "@lucide/vue/dist/esm/icons/x.mjs";
import { computed, nextTick, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { Button, Dialog, IconButton, Input, ListItem, Popover } from "../../../ui";
import { searchConversations } from "../conversationSearch";
import {
  cancelConversationDelete,
  confirmConversationDelete,
  dismissConversationError,
  ensureSidebarConversationsLoaded,
  sidebarConversationsState,
} from "../sidebarConversations";

const route = useRoute();
const router = useRouter();
const active = ref(false);
const query = ref("");
const selectedIndex = ref(0);
const inputRef = ref<{ $el?: HTMLInputElement } | null>(null);
const results = computed(() => searchConversations(sidebarConversationsState.rows, query.value).slice(0, 12));
const visibleError = computed(() => sidebarConversationsState.actionError ?? (active.value ? sidebarConversationsState.loadError : null));

watch(results, () => { selectedIndex.value = 0; });

async function setSearchOpen(open: boolean) {
  active.value = open;
  query.value = "";
  selectedIndex.value = 0;
  if (!open) return;
  void ensureSidebarConversationsLoaded().catch(() => undefined);
  await nextTick();
  inputRef.value?.$el?.focus();
}

function startConversation() {
  void setSearchOpen(false);
  void router.push("/");
}

function selectResult(result: (typeof results.value)[number]) {
  void setSearchOpen(false);
  void router.push(`/chats/${result.id}`);
}

function onSearchKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.preventDefault();
    void setSearchOpen(false);
  } else if (event.key === "ArrowDown" && results.value.length) {
    event.preventDefault();
    selectedIndex.value = (selectedIndex.value + 1) % results.value.length;
  } else if (event.key === "ArrowUp" && results.value.length) {
    event.preventDefault();
    selectedIndex.value = (selectedIndex.value - 1 + results.value.length) % results.value.length;
  } else if (event.key === "Enter") {
    event.preventDefault();
    const result = results.value[selectedIndex.value];
    if (result) selectResult(result);
  }
}

async function deleteSelectedConversation() {
  const deletedId = await confirmConversationDelete();
  if (deletedId && String(route.params.id ?? "") === deletedId) await router.push("/");
}
</script>

<template>
  <div class="conversation-sidebar-top">
    <div class="conversation-sidebar-top__actions">
      <Button :icon="MessageSquarePlus" class="conversation-sidebar-top__primary" agent-id="sidebar.new-chat" @click="startConversation">
        新对话
      </Button>
      <Popover :open="active" aria-label="搜索会话" placement="bottom" agent-id="sidebar.search" @update:open="setSearchOpen">
        <template #trigger><IconButton :icon="Search" label="搜索会话" agent-id="sidebar.search.open" /></template>
        <div class="conversation-search" @keydown="onSearchKeydown">
          <div class="conversation-search__input">
            <Input ref="inputRef" v-model="query" placeholder="搜索会话…" :spellcheck="false" agent-id="sidebar.search.input" />
            <IconButton :icon="X" label="关闭搜索" agent-id="sidebar.search.close" @click="setSearchOpen(false)" />
          </div>
          <div class="conversation-search__results" role="listbox" aria-label="会话搜索结果">
            <ListItem
              v-for="(result, index) in results"
              :key="result.id"
              :active="selectedIndex === index"
              :agent-id="`sidebar.search.result.${result.id}`"
              @mouseenter="selectedIndex = index"
              @select="selectResult(result)"
            >
              <span>{{ result.title }}</span><small>{{ result.projectName ?? "收集箱" }}</small>
            </ListItem>
            <p v-if="!results.length && sidebarConversationsState.loading">正在加载对话…</p>
            <p v-else-if="!results.length && query.trim()">没有匹配</p>
            <p v-else-if="!results.length">输入关键词</p>
          </div>
        </div>
      </Popover>
    </div>

    <div v-if="visibleError" class="conversation-sidebar-top__error" role="alert">
      <span>{{ visibleError }}</span>
      <IconButton :icon="X" label="忽略错误" agent-id="sidebar.error.dismiss" @click="dismissConversationError" />
    </div>
  </div>

  <Dialog
    :open="Boolean(sidebarConversationsState.deleteTarget)"
    title="删除对话"
    description="该操作无法撤销。"
    agent-id="sidebar.delete.dialog"
    :close-disabled="sidebarConversationsState.deleting"
    @update:open="!$event && cancelConversationDelete()"
    @close="cancelConversationDelete"
  >
    <p>永久删除“{{ sidebarConversationsState.deleteTarget?.title ?? "" }}”？消息、计划和工具记录将无法恢复；已生成的记忆会保留。</p>
    <template #actions>
      <Button :disabled="sidebarConversationsState.deleting" @click="cancelConversationDelete">取消</Button>
      <Button variant="danger" :loading="sidebarConversationsState.deleting" agent-id="sidebar.delete.confirm" @click="deleteSelectedConversation">
        {{ sidebarConversationsState.deleting ? "正在删除" : "彻底删除" }}
      </Button>
    </template>
  </Dialog>
</template>

<style scoped>
.conversation-sidebar-top { width: 100%; min-width: 0; }
.conversation-sidebar-top__actions { display: flex; align-items: center; gap: 6px; }
.conversation-sidebar-top__primary { flex: 1; justify-content: flex-start; }
.conversation-sidebar-top__error { display: flex; align-items: center; gap: 4px; margin-top: 4px; padding: 4px 6px; border-radius: var(--radius-xs); background: var(--err-soft); color: var(--err); font-size: 11px; }
.conversation-sidebar-top__error span { flex: 1; min-width: 0; }
.conversation-search { display: grid; gap: 8px; width: min(320px, calc(100vw - 32px)); }
.conversation-search__input { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 6px; }
.conversation-search__results { display: grid; gap: 3px; max-height: 320px; overflow: auto; }
.conversation-search__results :deep(.ui-list-item) { display: flex; justify-content: space-between; gap: 12px; width: 100%; }
.conversation-search__results small, .conversation-search__results p { color: var(--text-muted); }
.conversation-search__results p { margin: 8px; font-size: 12px; }
</style>
