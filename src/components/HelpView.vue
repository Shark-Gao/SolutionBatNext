<script setup lang="ts">
import { computed } from 'vue';
import { marked } from 'marked';
import DOMPurify from 'dompurify';
import guide from '../docs/user-guide.md?raw';
import { helpUrl } from '../api';
const emit = defineEmits<{ openHelp: [] }>();
const html = computed(() => DOMPurify.sanitize(marked.parse(guide, { async: false })));
function openLink(event: MouseEvent) {
  const anchor = (event.target as HTMLElement).closest('a');
  if (anchor?.href === helpUrl) { event.preventDefault(); emit('openHelp'); }
}
</script>

<template><article class="help-document" aria-label="离线使用手册" @click="openLink" v-html="html" /></template>
