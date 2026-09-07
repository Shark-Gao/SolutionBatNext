<script setup lang="ts">
import { FolderOpen } from 'lucide-vue-next';
import IconButton from './IconButton.vue';
defineProps<{ label: string; modelValue: string; readonly?: boolean; browse?: boolean; placeholder?: string }>();
defineEmits<{ 'update:modelValue': [value: string]; browse: [] }>();
</script>
<template>
  <label class="field path-field">
    <span>{{ label }}</span>
    <span class="path-input" :class="{ readonly }">
      <input :aria-label="label" :value="modelValue" :readonly="readonly" :title="modelValue" :placeholder="placeholder || ''" @input="$emit('update:modelValue', ($event.target as HTMLInputElement).value)" />
      <IconButton v-if="browse && !readonly" :label="'选择' + label" @click.prevent="$emit('browse')"><FolderOpen :size="16" /></IconButton>
    </span>
  </label>
</template>
