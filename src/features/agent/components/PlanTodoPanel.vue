<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { ConversationPlan, PlanStep } from "../types";

const props = defineProps<{ plan: ConversationPlan }>();
const expanded = ref(false);

const incompleteSteps = computed(() =>
  props.plan.steps.filter((step) => step.status === "in_progress" || step.status === "pending"),
);
const completedCount = computed(() =>
  props.plan.steps.filter((step) => step.status === "completed").length,
);
const visibleSteps = computed(() => expanded.value ? props.plan.steps : incompleteSteps.value);
const statusLabels: Record<PlanStep["status"], string> = {
  pending: "待处理",
  in_progress: "进行中",
  completed: "已完成",
  cancelled: "已取消",
};

watch(
  () => props.plan.conversationId,
  () => { expanded.value = false; },
);

</script>

<template>
  <section class="plan-todo-panel">
    <header class="plan-todo-panel__header">
      <div class="plan-todo-panel__identity">
        <span class="plan-todo-panel__title">计划</span>
        <span class="plan-todo-panel__summary">{{ completedCount }}/{{ plan.steps.length }} 完成</span>
      </div>
      <button
        type="button"
        class="plan-todo-panel__toggle"
        data-agent-id="agent.chat.plan.toggle"
        :aria-expanded="expanded"
        @click="expanded = !expanded"
      >
        {{ expanded ? "仅看待办" : "查看全部" }}
      </button>
    </header>

    <p v-if="!visibleSteps.length" class="plan-todo-panel__complete" role="status">
      当前计划已完成。
    </p>
    <ol v-else class="plan-todo-panel__list">
      <li
        v-for="step in visibleSteps"
        :key="step.id"
        class="plan-todo-panel__step"
        :class="`is-${step.status}`"
        :data-agent-id="`agent.chat.plan.step.${step.id}`"
        :data-status="step.status"
      >
        <span class="plan-todo-panel__status" aria-hidden="true" />
        <span class="plan-todo-panel__text">{{ step.title }}</span>
        <span class="plan-todo-panel__label">{{ statusLabels[step.status] }}</span>
      </li>
    </ol>
  </section>
</template>

<style scoped>
.plan-todo-panel {
  max-height: 220px;
  overflow: auto;
  padding: 6px 8px;
  border: 1px solid var(--border);
  border-bottom: 0;
  border-radius: var(--radius-md) var(--radius-md) 0 0;
  background: var(--bg-elev);
  box-shadow: 0 4px 16px -10px rgba(0, 0, 0, 0.45);
}

.plan-todo-panel__header,
.plan-todo-panel__identity,
.plan-todo-panel__step {
  display: flex;
  align-items: center;
}

.plan-todo-panel__header {
  min-height: 26px;
  justify-content: space-between;
  gap: 8px;
  padding: 0 4px 3px;
}

.plan-todo-panel__identity { min-width: 0; gap: 8px; }
.plan-todo-panel__title { color: var(--text); font-size: 12px; font-weight: 600; }
.plan-todo-panel__summary { color: var(--text-faint); font-size: 11px; }
.plan-todo-panel__toggle { padding: 2px 4px; border: 0; border-radius: var(--radius-xs); background: transparent; color: var(--text-muted); cursor: pointer; font-size: 11px; }
.plan-todo-panel__toggle:hover { background: var(--bg-hover); color: var(--text); }

.plan-todo-panel__list { display: flex; flex-direction: column; gap: 2px; margin: 0; padding: 0; list-style: none; }
.plan-todo-panel__step { gap: 7px; min-height: 27px; padding: 3px 4px; border-radius: var(--radius-sm); font-size: 12px; }
.plan-todo-panel__step:hover { background: var(--bg-hover); }
.plan-todo-panel__status { width: 8px; height: 8px; flex: 0 0 auto; border: 1px solid var(--border-strong); border-radius: 50%; background: transparent; }
.plan-todo-panel__step.is-in_progress .plan-todo-panel__status { border-color: var(--warn); background: var(--warn); }
.plan-todo-panel__step.is-completed .plan-todo-panel__status { border-color: var(--ok); background: var(--ok); }
.plan-todo-panel__step.is-cancelled .plan-todo-panel__status { border-color: var(--text-faint); background: var(--text-faint); }
.plan-todo-panel__text { min-width: 0; flex: 1 1 auto; line-height: 1.4; overflow-wrap: anywhere; }
.plan-todo-panel__step.is-completed .plan-todo-panel__text,
.plan-todo-panel__step.is-cancelled .plan-todo-panel__text { color: var(--text-muted); text-decoration: line-through; }
.plan-todo-panel__label { flex: 0 0 auto; color: var(--text-faint); font-size: 10px; }
.plan-todo-panel__complete { margin: 0; padding: 4px; color: var(--ok); font-size: 12px; }
</style>
