import { getIdentifier, getVersion } from "@tauri-apps/api/app";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { resolveUpdatePolicy } from "./updatePolicy";

export type UpdateCheck = { kind: "beta" } | { kind: "current"; version: string } | { kind: "available"; update: Update };

export async function checkSkyDekUpdate(): Promise<UpdateCheck> {
  return resolveUpdatePolicy(await getIdentifier(), check, getVersion);
}
