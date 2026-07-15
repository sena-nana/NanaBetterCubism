<template>
  <section class="conversation-surface">
    <header v-if="$slots.header" class="conversation-surface__header">
      <slot name="header" />
    </header>

    <main class="conversation-surface__main">
      <slot />
    </main>

    <div v-if="$slots.context" class="conversation-surface__context">
      <slot name="context" />
    </div>

    <footer class="conversation-surface__footer">
      <slot name="composer" />
    </footer>
  </section>
</template>

<style scoped>
.conversation-surface {
  display: grid;
  grid-template-rows: auto minmax(0, 1fr) auto auto;
  width: 100%;
  height: 100%;
  min-height: 0;
  background: var(--bg);
}

.conversation-surface__header {
  min-width: 0;
  border-bottom: 1px solid var(--border-soft);
}

.conversation-surface__main {
  min-width: 0;
  min-height: 0;
}

.conversation-surface__context,
.conversation-surface__footer {
  min-width: 0;
  padding-inline: clamp(16px, 5vw, 64px);
}

.conversation-surface__context {
  padding-top: 8px;
}

.conversation-surface__footer {
  padding-top: 8px;
  padding-bottom: 16px;
}

@media (max-width: 720px) {
  .conversation-surface__context,
  .conversation-surface__footer {
    padding-inline: 12px;
  }
}
</style>
