<template>
  <section class="conversation-surface">
    <header v-if="$slots.header" class="conversation-surface__header">
      <slot name="header" />
    </header>

    <main class="conversation-surface__main">
      <slot />
    </main>

    <footer class="conversation-surface__controls">
      <div class="conversation-surface__controls-inner">
        <slot name="context" />
        <slot name="composer" />
      </div>
    </footer>
  </section>
</template>

<style scoped>
.conversation-surface {
  display: grid;
  grid-template-areas:
    "header"
    "main"
    "controls";
  grid-template-rows: auto minmax(0, 1fr) auto;
  width: 100%;
  height: 100%;
  min-height: 0;
  background: var(--bg);
}

.conversation-surface__header { grid-area: header; min-width: 0; border-bottom: 1px solid var(--border-soft); }
.conversation-surface__main { grid-area: main; min-width: 0; min-height: 0; }
.conversation-surface__controls { grid-area: controls; position: relative; z-index: 3; min-width: 0; padding: 10px clamp(16px, 5vw, 64px) 14px; background: var(--bg); }
.conversation-surface__controls-inner { display: flex; flex-direction: column; width: min(860px, 100%); margin: 0 auto; }
.conversation-surface__controls-inner :deep(.context-panel) { margin-bottom: 8px; }
.conversation-surface__controls-inner :deep(.plan-todo-panel + .conversation-composer) { border-top-left-radius: 0; border-top-right-radius: 0; border-top-color: var(--border-soft); }

@media (max-width: 720px) {
  .conversation-surface__controls { padding: 8px 12px 10px; }
}
</style>
