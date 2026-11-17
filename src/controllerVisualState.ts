export type ControllerInputState = {
  buttons: Record<string, boolean>;
  left_stick_x: number; left_stick_y: number; right_stick_x: number; right_stick_y: number;
  left_trigger: number; right_trigger: number;
};

export function controlIntensity(input: ControllerInputState | null, control: string): number {
  if (!input) return 0;
  if (control === "lt") return input.left_trigger;
  if (control === "rt") return input.right_trigger;
  return input.buttons[control] ? 1 : 0;
}

export function stickDisplacement(input: ControllerInputState | null, control: string): [number, number] {
  return control === "l3" ? [input?.left_stick_x || 0, input?.left_stick_y || 0]
    : control === "r3" ? [input?.right_stick_x || 0, input?.right_stick_y || 0] : [0, 0];
}
