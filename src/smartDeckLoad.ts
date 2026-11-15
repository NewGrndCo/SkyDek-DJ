export type SmartDeckState = {
  loaded: readonly (string | null)[];
  playing: readonly boolean[];
  lastSelected: number;
};

/** Select a safe destination for an implicit library load. */
export function chooseSmartLoadDeck(state: SmartDeckState): number | null {
  for (let deck = 0; deck < 2; deck += 1) {
    if (!state.loaded[deck] && !state.playing[deck]) return deck;
  }

  const stopped = [0, 1].filter((deck) => !state.playing[deck]);
  if (stopped.length === 0) return null;
  if (stopped.length === 1) return stopped[0];
  return state.lastSelected === 1 ? 1 : 0;
}
