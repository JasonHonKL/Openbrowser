import { invoke } from "@tauri-apps/api/core";
import type { InstanceInfo } from "./types";

export async function listInstances(): Promise<InstanceInfo[]> {
  return invoke<InstanceInfo[]>("list_instances");
}

export async function spawnInstance(): Promise<InstanceInfo> {
  return invoke<InstanceInfo>("spawn_instance");
}

export async function killInstance(id: string): Promise<void> {
  return invoke<void>("kill_instance", { id });
}

export async function killAllInstances(): Promise<void> {
  return invoke<void>("kill_all_instances");
}

export async function openChallengeWindow(
  url: string,
  title?: string
): Promise<string> {
  return invoke<string>("open_challenge_window", { url, title });
}

export async function submitChallengeResolution(
  challengeUrl: string,
  cookies: string,
  headers: Record<string, string> = {}
): Promise<void> {
  return invoke<void>("submit_challenge_resolution", {
    challengeUrl,
    cookies,
    headers,
  });
}

export async function cancelChallenge(challengeUrl: string): Promise<void> {
  return invoke<void>("cancel_challenge", { challengeUrl });
}
