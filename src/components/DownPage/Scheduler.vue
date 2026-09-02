<template>
  <div class="wrapper">
    <div class="flex mb-2.5 mx-[14px] text items-center">
      <i :class="[$fa.weight, 'fa-id-badge']"></i>
      <span class="mr-auto">{{ sche.sid }} ({{ sche.list.length }})</span>
      <div class="flex gap-2 items-center">
        <span class="mr-2 ml-auto">{{ timestamp(sche.ts) }}</span>
        <button v-for="(v, k) in buttons" :key="k" @click="event(k)">
          <i :class="[$fa.weight, v]"></i>
        </button>
      </div>
    </div>
    <Empty v-if="!tasks.length" :text="$t('down.empty')" />
    <VList
      v-slot="{ item }"
      :data="tasks"
      class="contain-paint!"
    >
      <Task :sid="sche.sid" :task="item" />
    </VList>
  </div>
</template>

<script lang="ts" setup>
import { commands, CtrlEvent } from '@/services/backend';
import { timestamp } from '@/services/utils';
import { useQueueStore } from '@/store';
import { computed } from 'vue';

import * as Types from '@/types/shared.d';
import { VList } from 'virtua/vue';
import { Empty } from '@/components';
import Task from './Task.vue';

const props = defineProps<{
  sche: Types.Scheduler;
  dispatch: () => void;
}>();

const queue = useQueueStore();

const tasks = computed(() =>
  props.sche.list
    .map((v) => queue.tasks[v])
    .filter((v): v is Types.Task => Boolean(v)),
);

const buttons = computed(() => ({
  ...(props.sche.state === 'idle' && {
    start: 'fa-play',
  }),
  ...(props.sche.state === 'running' && {
    pause: 'fa-pause',
  }),
  ...(props.sche.state === 'paused' && {
    resume: 'fa-play',
  }),
  openFolder: 'fa-folder-open',
  cancel: 'fa-trash',
}));

async function event(event: CtrlEvent | 'openFolder' | 'start') {
  const sid = props.sche.sid;
  const result =
    event === 'openFolder'
      ? await commands.openFolder(sid, null)
      : event === 'start'
        ? await commands.ctrlEvent('resume', sid, null)
      : await commands.ctrlEvent(event, sid, null);
  if (result.status === 'error') throw result.error;
}
</script>
