import { computed, onMounted, onUnmounted, ref, watch, type Ref } from 'vue';

const storageKey = 'solutionbat-next-log-height-v1';

export function useLogPanelResize(enabled: Ref<boolean>) {
  const mainArea = ref<HTMLElement>();
  const logPanel = ref<HTMLElement>();
  const preferredHeight = ref<number>();
  const actualHeight = ref(150);
  const minHeight = ref(150);
  const maxHeight = ref(600);
  const resizing = ref(false);
  let observer: ResizeObserver | undefined;
  let drag: { target: HTMLElement; pointerId: number; y: number; height: number; previous?: number } | undefined;

  try {
    const stored = Number(localStorage.getItem(storageKey));
    if (Number.isFinite(stored) && stored >= 100 && stored <= 10000) preferredHeight.value = stored;
  } catch { /* Resizing remains available when local storage is unavailable. */ }

  const clamp = (value: number) => Math.round(Math.min(maxHeight.value, Math.max(minHeight.value, value)));
  const panelStyle = computed(() => enabled.value && preferredHeight.value !== undefined
    ? { flexBasis: `${clamp(preferredHeight.value)}px`, height: `${clamp(preferredHeight.value)}px` }
    : undefined);

  function measure() {
    const main = mainArea.value;
    if (!main) return;
    const chromeHeight = ['.topbar', '.preview-banner', '.statusbar'].reduce((sum, selector) => sum + (main.querySelector(selector)?.getBoundingClientRect().height || 0), 0);
    const hostHeight = window.innerWidth <= 600 ? window.innerHeight : main.getBoundingClientRect().height;
    maxHeight.value = Math.max(100, Math.floor(hostHeight - chromeHeight - 140));
    minHeight.value = Math.min(150, maxHeight.value);
    actualHeight.value = Math.round(logPanel.value?.getBoundingClientRect().height || 150);
  }

  function saveHeight() {
    try {
      if (preferredHeight.value === undefined) localStorage.removeItem(storageKey);
      else localStorage.setItem(storageKey, String(preferredHeight.value));
    } catch { /* Keep the current size even when it cannot be persisted. */ }
  }

  function finish(cancel = false) {
    const active = drag;
    if (!active) return;
    drag = undefined;
    resizing.value = false;
    document.documentElement.classList.remove('resizing-logs');
    if (cancel) preferredHeight.value = active.previous;
    else saveHeight();
    if (active.target.hasPointerCapture(active.pointerId)) active.target.releasePointerCapture(active.pointerId);
  }

  function startResize(event: PointerEvent) {
    if (!enabled.value || event.button !== 0 || !event.isPrimary || drag) return;
    measure();
    const target = event.currentTarget as HTMLElement;
    target.focus({ preventScroll: true });
    target.setPointerCapture(event.pointerId);
    drag = { target, pointerId: event.pointerId, y: event.clientY, height: actualHeight.value, previous: preferredHeight.value };
    resizing.value = true;
    document.documentElement.classList.add('resizing-logs');
    event.preventDefault();
  }

  function moveResize(event: PointerEvent) {
    if (!drag || drag.pointerId !== event.pointerId) return;
    preferredHeight.value = clamp(drag.height + drag.y - event.clientY);
    event.preventDefault();
  }

  function endResize(event: PointerEvent) {
    if (drag?.pointerId === event.pointerId) finish();
  }

  function cancelResize() { finish(true); }

  function resetHeight() {
    if (!enabled.value) return;
    cancelResize();
    preferredHeight.value = undefined;
    saveHeight();
  }

  function resizeKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') { cancelResize(); return; }
    if (!enabled.value || !['ArrowUp', 'ArrowDown', 'Home', 'End'].includes(event.key)) return;
    event.preventDefault();
    measure();
    const step = event.shiftKey ? 40 : 10;
    preferredHeight.value = event.key === 'Home' ? minHeight.value : event.key === 'End' ? maxHeight.value : clamp(actualHeight.value + (event.key === 'ArrowUp' ? step : -step));
    saveHeight();
  }

  watch([mainArea, logPanel], () => {
    observer?.disconnect();
    observer = new ResizeObserver(measure);
    for (const element of [mainArea.value, logPanel.value]) if (element) observer.observe(element);
    measure();
  }, { flush: 'post' });
  watch(enabled, value => { if (!value) cancelResize(); });
  onMounted(() => { window.addEventListener('resize', measure); window.addEventListener('blur', cancelResize); });
  onUnmounted(() => { cancelResize(); observer?.disconnect(); window.removeEventListener('resize', measure); window.removeEventListener('blur', cancelResize); });

  return { mainArea, logPanel, panelStyle, actualHeight, minHeight, maxHeight, resizing, startResize, moveResize, endResize, cancelResize, resetHeight, resizeKeydown };
}
