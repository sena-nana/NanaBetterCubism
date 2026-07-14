<script setup lang="ts">
import EditorConnectionCard from "../../editor/EditorConnectionCard.vue";
</script>

<template>
  <section class="settings-section" data-agent-id="settings.editor">
    <header class="page-header">
      <h1>Editor</h1>
      <p>连接本机 Cubism Editor External API。Agent 工具依赖此连接。</p>
    </header>
    <EditorConnectionCard agent-id-prefix="settings.editor.connection" />
  </section>
</template>
