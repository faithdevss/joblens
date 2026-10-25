/** Ref callback that focuses an element once, when it mounts (e.g. a dialog's safe button). */
export function focusOnMount(el: HTMLElement | null) {
  el?.focus();
}
