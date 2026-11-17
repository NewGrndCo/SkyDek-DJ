const key = "skydek-controller-callouts";

export function readCalloutPreference(storage: Pick<Storage, "getItem">): boolean {
  try { return storage.getItem(key) !== "false"; }
  catch { return true; }
}

export function saveCalloutPreference(storage: Pick<Storage, "setItem">, enabled: boolean): void {
  try { storage.setItem(key, String(enabled)); }
  catch { /* Optional local preference. */ }
}
