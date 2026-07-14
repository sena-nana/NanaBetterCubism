<script setup lang="ts">
import { APP_SHELL_COPY } from "@lilia/ui";
</script>

<template>
  <section data-agent-id="home.page">
    <div class="page-header" data-agent-id="home.header">
      <div>
        <h1>{{ APP_SHELL_COPY.homeTitle }}</h1>
        <p>{{ APP_SHELL_COPY.homeDescription }}</p>
      </div>
    </div>

    <div class="card" data-agent-id="home.start-card">
      <h2>开始</h2>
      <p class="muted">这是应用的默认首页。</p>
    </div>
  </section>
</template>
