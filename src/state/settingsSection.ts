/** A Settings section another page asked to open (Settings reads it when it next appears). */
let requested: string | null = null;

export function requestSettingsSection(id: string) {
  requested = id;
}

export function takeRequestedSettingsSection(): string | null {
  const id = requested;
  requested = null;
  return id;
}
