<script setup lang="ts">
defineProps<{ label: string; active?: boolean }>();
</script>
<template>
  <button type="button" class="icon-button" :class="{ active }" :aria-label="label" :title="label"><slot /></button>
</template>
