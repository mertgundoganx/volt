<script setup lang="ts">
const store = useCollectionStore()
const report = computed(() => store.importReport!)

function close() {
  store.importReport = null
}

// Written inline, the braces would close the template interpolation early.
const placeholderExample = '{{name}}'
</script>

<template>
  <UiDialog :title="`Imported from ${report.format}`" eyebrow="Import complete" :width="620" @close="close">
    <p class="path mono selectable" :title="report.path">
      <UiIcon name="folder" :size="14" />{{ report.path }}
    </p>

    <div class="readout">
      <UiMeasure label="Requests" :value="report.requests" />
      <UiMeasure label="Folders" :value="report.folders" />
      <UiMeasure label="Environments" :value="report.environments.length" />
      <UiMeasure label="Secrets" :value="report.secrets.length" />
    </div>

    <section v-if="report.secrets.length" class="block">
      <h3 class="silk">Kept out of the YAML</h3>
      <p>
        These values went to gitignored <code>.env.&lt;environment&gt;</code> files. That includes
        credentials the export had in plain text; the requests now read
        <code>{{ placeholderExample }}</code> in their place.
      </p>
      <div class="chips">
        <span v-for="name in report.secrets" :key="name" class="chip"><UiIcon name="lock" :size="12" />{{ name }}</span>
      </div>
    </section>

    <section v-if="report.warnings.length" class="block">
      <h3 class="silk">Worth checking</h3>
      <ul class="warnings">
        <li v-for="warning in report.warnings" :key="warning">
          <span class="led warn" />
          <span>{{ warning }}</span>
        </li>
      </ul>
    </section>
    <section v-else class="block">
      <p class="clean"><UiIcon name="check" :size="14" />Everything in the export had a direct equivalent.</p>
    </section>

    <template #footer>
      <span class="spacer" />
      <button type="button" class="btn btn-primary" autofocus @click="close">Done</button>
    </template>
  </UiDialog>
</template>

<style scoped>
.path {
  display: flex;
  align-items: center;
  gap: var(--s-2);
  margin: 0;
  color: var(--silk);
  font-size: var(--t-meta);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.readout {
  display: flex;
  gap: var(--s-8);
  margin-top: var(--s-4);
  padding: var(--s-4) 0;
  border-top: 1px solid var(--line-soft);
  border-bottom: 1px solid var(--line-soft);
}

.block { margin-top: var(--s-5); }
.block h3 { margin: 0 0 var(--s-2); }
.block p { margin: 0 0 var(--s-3); color: var(--ink-2); font-size: var(--t-small); line-height: 1.55; }
code { font-family: var(--font-mono); font-size: 0.95em; }

.chips { display: flex; flex-wrap: wrap; gap: 6px; }

.warnings { list-style: none; margin: 0; padding: 0; display: grid; gap: var(--s-2); }
.warnings li { display: flex; gap: var(--s-3); align-items: flex-start; font-size: var(--t-small); line-height: 1.5; color: var(--ink-2); }
.warnings .led { margin-top: 6px; }

.clean { display: flex; align-items: center; gap: var(--s-2); color: var(--ok) !important; }
</style>
