<script setup lang="ts">
import { CheckCircle2, CircleAlert, Clock3, Plus, RefreshCw, Trash2 } from 'lucide-vue-next';
import { computed } from 'vue';
import IconButton from './IconButton.vue';
import type { ScheduleInfo } from '../types';

const props = defineProps<{ info?: ScheduleInfo; error?: string; busy: boolean }>();
const times = defineModel<string[]>({ required: true });
const emit = defineEmits<{ register: []; unregister: []; refresh: [] }>();
const changed = computed(() => props.info?.registered && JSON.stringify([...times.value].sort()) !== JSON.stringify([...props.info.times].sort()));
function setTime(index: number, event: Event) {
  times.value = times.value.map((time, i) => i === index ? (event.target as HTMLInputElement).value : time);
}
</script>

<template>
  <section class="schedule-panel" aria-label="Windows 计划任务">
    <div class="schedule-status-band">
      <CheckCircle2 v-if="info?.registered && !error && info.state !== 'Disabled'" :size="24" />
      <CircleAlert v-else-if="error" :size="24" class="danger-text" />
      <Clock3 v-else :size="24" class="muted" />
      <div><strong>{{ error ? '状态查询失败' : !info ? '正在检查计划任务' : info.registered ? info.state === 'Disabled' ? '计划任务已禁用' : '计划任务已注册' : '计划任务未注册' }}</strong><p>{{ info?.registered && info.nextRun ? `下次运行：${info.nextRun}` : info?.name || 'Windows 任务计划程序' }}</p></div>
      <IconButton label="刷新计划状态" :disabled="busy" @click="emit('refresh')"><RefreshCw :size="16" /></IconButton>
    </div>
    <div v-if="error" class="notice error"><CircleAlert :size="16" /><span>{{ error }}</span></div>
    <div v-if="changed" class="notice warning"><CircleAlert :size="16" /><span>触发时间尚未注册，当前已注册：{{ info?.times.join('、') || '无' }}</span></div>
    <div v-if="info?.previousRegistered" class="notice warning"><CircleAlert :size="16" /><span>检测到上一版兼容任务：{{ info.name }}</span></div>
    <fieldset :disabled="busy" class="form-section">
      <div class="section-heading"><h2>触发时间</h2><span class="count">{{ times.length }}</span><button class="button compact" :disabled="times.length >= 24" @click="times = [...times, '08:00']"><Plus :size="15" />添加时间</button></div>
      <div class="trigger-table" role="table" aria-label="每日触发时间">
        <div class="table-head" role="row"><span role="columnheader">触发器</span><span role="columnheader">执行时间</span><span role="columnheader">重复</span><span role="columnheader" aria-label="操作" /></div>
        <div v-for="(time, index) in times" :key="index" class="trigger-row" role="row">
          <span class="trigger-name" role="cell"><Clock3 :size="15" />触发器 {{ index + 1 }}</span>
          <span role="cell"><input type="time" step="60" :value="time" :aria-label="`触发时间 ${index + 1}`" @input="setTime(index, $event)" /></span>
          <span class="muted small" role="cell">每天</span>
          <span role="cell"><IconButton :label="`删除触发时间 ${index + 1}`" @click="times = times.filter((_, i) => i !== index)"><Trash2 :size="16" /></IconButton></span>
        </div>
        <div v-if="!times.length" class="table-empty">尚未添加触发时间</div>
      </div>
    </fieldset>
    <div class="schedule-actions"><button class="button primary" :disabled="busy" @click="emit('register')"><Clock3 :size="16" />注册计划任务</button><button class="button" :disabled="busy" @click="emit('unregister')"><Trash2 :size="16" />删除计划任务</button></div>
    <dl v-if="info" class="schedule-details"><div><dt>任务名称</dt><dd>{{ info.name }}</dd></div><div v-if="info.executable"><dt>执行程序</dt><dd>{{ info.executable }}</dd></div></dl>
  </section>
</template>
