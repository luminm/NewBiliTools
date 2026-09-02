<template>
  <div>
    <div class="flex w-full my-1.5 items-center gap-2">
      <h1 class="flex-1">
        <i :class="[$fa.weight, 'fa-download']"></i>
        <span>{{ $t('down.title') }}</span>
      </h1>
      <button
        v-if="tab !== 'backlog' && canStart"
        class="primary-color w-fit"
        @click="startAll"
      >
        <i :class="[$fa.weight, 'fa-play']"></i>
        <span>{{ $t('down.processQueue') }}</span>
      </button>
    </div>
    <div class="flex w-full h-full mt-4 flex-1 gap-3 min-h-0">
      <Transition mode="out-in">
        <Queue :key="tab" v-model="tab" />
      </Transition>
      <div class="tab">
        <button
          v-for="(i, k) in tabs"
          :key="k"
          :class="{ active: tab === k }"
          @click="tab = k"
        >
          <span>{{ $t(`down.${k}`) }}</span>
          <i :class="[tab === k ? 'fa-solid' : 'fa-light', i]"></i>
          <label class="primary-color"></label>
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue';

import { useQueueStore } from '@/store';
import { Queue } from '@/components/DownPage';
import { processQueue } from '@/services/queue';
import { commands } from '@/services/backend';

const tabs = {
  backlog: 'fa-books-medical',
  pending: 'fa-stopwatch',
  doing: 'fa-hourglass-half',
  complete: 'fa-check',
};
type Tab = keyof typeof tabs;

const tab = ref<Tab>('backlog');

const queue = useQueueStore();

const canStart = computed(() => {
  if (queue.backlog.length) return true;
  return queue.doing.some((sid) => {
    const state = queue.schedulers[sid]?.state;
    return state === 'idle' || state === 'paused';
  });
});

async function startAll() {
  tab.value = 'doing';
  if (queue.backlog.length) {
    await processQueue();
  }
  for (const sid of queue.doing) {
    const state = queue.schedulers[sid]?.state;
    if (state !== 'idle' && state !== 'paused') continue;
    const result = await commands.ctrlEvent('resume', sid, null);
    if (result.status === 'error') throw result.error;
  }
}

defineExpose({ tab });
</script>

<style scoped>
@reference 'tailwindcss';

:deep(.wrapper) {
  @apply flex flex-col p-3 rounded-lg text-sm bg-(--block-color);
  @apply gap-0.5 my-px border border-(--split-color) w-full min-h-0;
}
</style>
