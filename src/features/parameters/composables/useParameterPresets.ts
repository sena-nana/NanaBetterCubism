import { computed, reactive, ref } from "vue";
import { readJsonStorage, writeJsonStorage } from "../../../utils/storage";
import type { IdPreset, IdTemplateConfig } from "../types";
import { copyTemplate } from "../utils/idTemplate";

const STORAGE_KEY = "nanabettercubism.parameter-id-presets";

export const builtInPreset: IdPreset = {
  id: "builtin-default",
  name: "标准 Param 模板",
  template: "Param{key}{side}{index}",
  prefix: "Param",
  suffix: "",
  startIndex: 1,
  indexWidth: 2,
  builtIn: true,
};

export function useParameterPresets() {
  const stored = readJsonStorage<IdPreset[]>(STORAGE_KEY, []);
  const customPresets = ref(Array.isArray(stored) ? stored : []);
  const selectedPresetId = ref(builtInPreset.id);
  const presetName = ref("");
  const idTemplate = reactive<IdTemplateConfig>(copyTemplate(builtInPreset));
  const presets = computed(() => [builtInPreset, ...customPresets.value]);
  const presetOptions = computed(() => presets.value.map(({ id: value, name: label }) => ({ value, label })));
  const selectedPreset = computed(() => presets.value.find(({ id }) => id === selectedPresetId.value));

  function persist() {
    writeJsonStorage(STORAGE_KEY, customPresets.value);
  }

  function choosePreset(id: string) {
    selectedPresetId.value = id;
    const preset = presets.value.find((item) => item.id === id);
    if (!preset) return;
    Object.assign(idTemplate, copyTemplate(preset));
    presetName.value = preset.builtIn ? "" : preset.name;
  }

  function savePreset() {
    const name = presetName.value.trim();
    if (!name) return;
    const preset: IdPreset = { id: `preset-${Date.now()}`, name, ...copyTemplate(idTemplate) };
    customPresets.value.push(preset);
    selectedPresetId.value = preset.id;
    persist();
  }

  function renamePreset() {
    const name = presetName.value.trim();
    if (!name || selectedPreset.value?.builtIn) return;
    const preset = customPresets.value.find(({ id }) => id === selectedPresetId.value);
    if (!preset) return;
    preset.name = name;
    persist();
  }

  function deletePreset() {
    if (selectedPreset.value?.builtIn) return;
    customPresets.value = customPresets.value.filter(({ id }) => id !== selectedPresetId.value);
    choosePreset(builtInPreset.id);
    persist();
  }

  return {
    idTemplate,
    selectedPresetId,
    presetName,
    presetOptions,
    selectedPreset,
    choosePreset,
    savePreset,
    renamePreset,
    deletePreset,
  };
}
