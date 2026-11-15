export type ControllerMappings = {
  normal: Record<string, string>;
  shift: Record<string, string>;
};

export const mappingActionLabels: Record<string, string> = {
  auto_cross: "Auto crossfade",
  none: "Unmapped",
  cue_hold: "Cue / Hold",
  play: "Play / Pause",
  set_cue: "Set cue",
  load_selected: "Load selected",
  load_next: "Load next Sidelist",
  load_1: "Load to DEK 1",
  load_2: "Load to DEK 2",
  queue_add: "Add to Sidelist",
  browse_up: "Browse up",
  browse_down: "Browse down",
  panel: "Switch panel",
  backspin: "Backspin",
  brake: "Brake",
  start: "Brake start",
  echo: "Echo",
  reverb: "Reverb",
  sample: "Play sample",
  next_sample: "Next sample",
  stop_samples: "Stop samples",
  target_next: "Switch target DEK",
  auto_target: "Auto target",
  shift: "Normal / Shift",
  tempo_match: "Sync tempo",
  cross: "Crossfader",
  filter: "Filter",
  wet: "Effect mix",
  volume: "DEK volume",
  master: "Master volume",
};

export function mappingLabel(action: string | undefined): string {
  return mappingActionLabels[action || "none"] || "Unmapped";
}

export function isEssentialMapping(action: string | undefined): boolean {
  return action === "play" || action === "cue_hold" || action === "set_cue";
}
