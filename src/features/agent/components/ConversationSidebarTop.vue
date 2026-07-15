<script setup lang="ts">
import MessageSquarePlus from "@lucide/vue/dist/esm/icons/message-square-plus.mjs";
import Search from "@lucide/vue/dist/esm/icons/search.mjs";
import X from "@lucide/vue/dist/esm/icons/x.mjs";
import { ConfirmDialog, SearchDropdown } from "@lilia/ui";
import { computed, nextTick, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
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
const inputRef = ref<{ focus: () => void } | null>(null);

const results = computed(() =>
  searchConversations(sidebarConversationsState.rows, query.value).slice(0, 12),
);
const visibleError = computed(() =>
  sidebarConversationsState.actionError
  ?? (active.value ? sidebarConversationsState.loadError : null),
);

watch(results, () => {
  selectedIndex.value = 0;
});

async function openSearch() {
  active.value = true;
  query.value = "";
  selectedIndex.value = 0;
  void ensureSidebarConversationsLoaded().catch(() => undefined);
  await nextTick();
  inputRef.value?.focus();
}

function closeSearch() {
  active.value = false;
  query.value = "";
  selectedIndex.value = 0;
}

function startConversation() {
  closeSearch();
  void router.push("/");
}

function selectResult(result: (typeof results.value)[number]) {
  closeSearch();
  void router.push(`/chats/${result.id}`);
}

function onSearchKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.preventDefault();
    closeSearch();
    return;
  }
  if (event.key === "ArrowDown" && results.value.length) {
    event.preventDefault();
    selectedIndex.value = (selectedIndex.value + 1) % results.value.length;
  } else if (event.key === "ArrowUp" && results.value.length) {
    event.preventDefault();
    selectedIndex.value = (
      selectedIndex.value - 1 + results.value.length
    ) % results.value.length;
  } else if (event.key === "Enter") {
    event.preventDefault();
    const result = results.value[selectedIndex.value];
    if (result) selectResult(result);
  }
}

async function deleteSelectedConversation() {
  const deletedId = await confirmConversationDelete();
  if (deletedId && String(route.params.id ?? "") === deletedId) {
    await router.push("/");
  }
}
</script>

<template>
  <div class="conversation-sidebar-top">
    <div v-if="!active" class="conversation-sidebar-top__actions">
      <button
        type="button"
        class="conversation-sidebar-top__primary"
        data-agent-id="sidebar.new-chat"
        title="新对话"
        aria-label="新对话"
        @click="startConversation"
      >
        <MessageSquarePlus :size="15" aria-hidden="true" />
        <span>新对话</span>
      </button>
      <button
        type="button"
        class="conversation-sidebar-top__icon"
        data-agent-id="sidebar.search.open"
        title="搜索会话"
        aria-label="搜索会话"
        @click="openSearch"
      >
        <Search :size="15" aria-hidden="true" />
      </button>
    </div>

    <SearchDropdown
      v-else
      ref="inputRef"
      v-model="query"
      class="sidebar-search-dropdown"
      placeholder="搜索会话…"
      input-agent-id="sidebar.search.input"
      :spellcheck="false"
      @keydown="onSearchKeydown"
    >
      <template #leading>
        <Search :size="14" aria-hidden="true" class="search-dropdown__leading" />
      </template>
      <template #trailing>
        <button
          type="button"
          class="search-dropdown__action"
          data-agent-id="sidebar.search.close"
          title="关闭搜索 (Esc)"
          aria-label="关闭搜索"
          @click="closeSearch"
        >
          <X :size="13" aria-hidden="true" />
        </button>
      </template>
      <template #default="{ highlightRangeSegments }">
        <template v-if="results.length">
          <button
            v-for="(result, index) in results"
            :key="result.id"
            type="button"
            class="search-dropdown__item"
            :class="{ 'is-active': selectedIndex === index }"
            :data-agent-id="`sidebar.search.result.${result.id}`"
            role="option"
            :aria-selected="selectedIndex === index"
            @mouseenter="selectedIndex = index"
            @click="selectResult(result)"
          >
            <span class="search-dropdown__title">
              <template
                v-for="(segment, segmentIndex) in highlightRangeSegments(result.title, result.highlights)"
                :key="segmentIndex"
              >
                <mark v-if="segment.mark">{{ segment.text }}</mark>
                <template v-else>{{ segment.text }}</template>
              </template>
            </span>
            <span class="search-dropdown__scope">{{ result.projectName ?? "收集箱" }}</span>
          </button>
        </template>
        <p v-else-if="sidebarConversationsState.loading" class="search-dropdown__hint">
          正在加载对话…
        </p>
        <p v-else-if="query.trim()" class="search-dropdown__empty">没有匹配</p>
        <p v-else class="search-dropdown__hint">输入关键词</p>
      </template>
    </SearchDropdown>

    <div v-if="visibleError" class="conversation-sidebar-top__error" role="alert">
      <span>{{ visibleError }}</span>
      <button
        type="button"
        data-agent-id="sidebar.error.dismiss"
        title="忽略错误"
        aria-label="忽略错误"
        @click="dismissConversationError"
      >
        <X :size="12" aria-hidden="true" />
      </button>
    </div>
  </div>

  <ConfirmDialog
    :open="Boolean(sidebarConversationsState.deleteTarget)"
    title="删除对话"
    :message="`永久删除“${sidebarConversationsState.deleteTarget?.title ?? ''}”？消息、计划和工具记录将无法恢复；已生成的记忆会保留。`"
    confirm-text="彻底删除"
    busy-text="正在删除"
    :busy="sidebarConversationsState.deleting"
    danger
    @cancel="cancelConversationDelete"
    @confirm="deleteSelectedConversation"
  />
</template>

<style scoped>
.conversation-sidebar-top {
  width: 100%;
  min-width: 0;
}

.conversation-sidebar-top__actions {
  display: flex;
  align-items: center;
  gap: 6px;
}

.conversation-sidebar-top__primary,
.conversation-sidebar-top__icon {
  height: 30px;
  border: 0;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text);
  cursor: pointer;
  transition: background-color 0.12s ease, color 0.12s ease;
}

.conversation-sidebar-top__primary {
  flex: 1;
  min-width: 0;
  display: grid;
  grid-template-columns: 16px minmax(0, 1fr);
  align-items: center;
  gap: 6px;
  padding: 0 8px;
  font-size: 13px;
  font-weight: 500;
  text-align: left;
}

.conversation-sidebar-top__primary span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.conversation-sidebar-top__icon {
  flex: 0 0 30px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: var(--text-muted);
}

.conversation-sidebar-top__primary:hover,
.conversation-sidebar-top__icon:hover {
  background: var(--bg-hover);
  color: var(--text);
}

.conversation-sidebar-top__error {
  min-height: 24px;
  margin-top: 4px;
  padding: 4px 6px;
  display: flex;
  align-items: center;
  gap: 4px;
  border-radius: var(--radius-xs);
  background: var(--err-soft);
  color: var(--err);
  font-size: 11px;
  line-height: 1.35;
}

.conversation-sidebar-top__error span {
  flex: 1;
  min-width: 0;
}

.conversation-sidebar-top__error button {
  flex: 0 0 auto;
  width: 20px;
  height: 20px;
  padding: 0;
  border: 0;
  border-radius: var(--radius-xs);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  color: inherit;
  cursor: pointer;
}

.conversation-sidebar-top__error button:hover {
  background: color-mix(in srgb, var(--err) 12%, transparent);
}

.sidebar-search-dropdown {
  width: 100%;
}
</style>
