<script setup lang="ts">
import { ICONS, type IconName } from '~/utils/icons'

withDefaults(defineProps<{ name: IconName; size?: number }>(), { size: 16 })
</script>

<template>
  <!-- The markup comes from the constant icon set, never from input. -->
  <svg
    class="ui-icon"
    :width="size"
    :height="size"
    viewBox="0 0 16 16"
    aria-hidden="true"
    focusable="false"
    v-html="ICONS[name]"
  />
</template>

<style>
.ui-icon {
  display: block;
  flex: none;
  fill: none;
  stroke: currentColor;
  stroke-width: 1.5;
  stroke-linecap: round;
  stroke-linejoin: round;
}
.ui-icon .fill { fill: currentColor; stroke: none; }
</style>
