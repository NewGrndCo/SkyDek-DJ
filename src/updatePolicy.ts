export const OFFICIAL_ID = "com.newground.skydek-dj";

export async function resolveUpdatePolicy<T>(identifier: string, checkSigned: () => Promise<T | null>, version: () => Promise<string>): Promise<{ kind: "beta" } | { kind: "current"; version: string } | { kind: "available"; update: T }> {
  if (identifier !== OFFICIAL_ID) return { kind: "beta" };
  const update = await checkSigned();
  return update ? { kind: "available", update } : { kind: "current", version: await version() };
}
