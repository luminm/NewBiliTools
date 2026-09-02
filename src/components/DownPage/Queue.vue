<template>
  <div
    v-if="model === 'backlog'"
    class="wrapper"
    :style="{
      height: getHeight(queue[model].length),
    }"
  >
    <div class="flex mb-2.5 mx-[14px] text items-center">
      <i class="fa-solid fa-books-medical"></i>
      <span>{{ $t('down.backlog') }} ({{ queue.backlog.length }})</span>
      <button
        v-if="queue[model].length"
        class="ml-auto w-fit primary-color"
        @click="dispatch"
      >
        <i :class="[$fa.weight, 'fa-download']"></i>
        <span>{{ $t('down.processQueue') }}</span>
      </button>
    </div>
    <VList
      v-if="backlogTasks.length"
      v-slot="{ item }"
      :data="backlogTasks"
    >
      <Task :task="item" />
    </VList>
    <Empty v-else :text="$t('down.backlogEmpty')" />
  </div>
  <VList
    v-else-if="model && schedulers.length"
    v-slot="{ item }"
    :data="schedulers"
  >
    <Scheduler
      :sche="item"
      :dispatch
      :style="{
        height: getHeight(item.list.length),
      }"
    />
  </VList>
  <Empty v-else class="mt-0" :text="$t('down.empty')" />
</template>

<script lang="ts" setup>
import { useQueueStore } from '@/store';
import { VList } from 'virtua/vue';
import { processQueue } from '@/services/queue';
import * as Types from '@/types/shared.d';

import { Empty } from '@/components';
import { Scheduler, Task } from '.';
import { computed, ref } from 'vue';
import { getCurrentWindow } from '@tauri-apps/api/window';

const model = defineModel<'backlog' | 'pending' | 'doing' | 'complete'>();

const queue = useQueueStore();

const backlogTasks = computed(() =>
  queue.backlog
    .map((v) => queue.tasks[v])
    .filter((v): v is Types.Task => Boolean(v)),
);

const schedulers = computed(() => {
  if (!model.value || model.value === 'backlog') return [] as Types.Scheduler[];
  return queue[model.value]
    .map((v) => queue.schedulers[v])
    .filter((v): v is Types.Scheduler => Boolean(v));
});

const windowHeight = ref(window.innerHeight);

getCurrentWindow().onResized((e) => {
  windowHeight.value = e.payload.height;
});

function dispatch() {
  model.value = 'doing';
  processQueue();
}

function getHeight(len: number) {
  const pad = 66;
  const unit = 116;
  const raw = pad + unit * (len || 2);
  const maxHeight = windowHeight.value - 92;
  return (raw > maxHeight ? maxHeight : raw) + 'px';
}
</script>
