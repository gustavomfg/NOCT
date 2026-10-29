import { useEffect, useRef, type RefObject } from 'react'

// Only the topmost dialog owns keyboard focus and Escape.
const dialogStack: HTMLElement[] = []

const focusable = 'button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), a[href], [tabindex]:not([tabindex="-1"])'

export function useDialogA11y<T extends HTMLElement>(onClose: () => void): RefObject<T | null> {
  const ref = useRef<T>(null)
  const closeRef = useRef(onClose)
  closeRef.current = onClose

  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null
    const dialog = ref.current
    if (!dialog) return
    dialogStack.push(dialog)
    const first = dialog.querySelector<HTMLElement>(focusable)
    const focusFrame = requestAnimationFrame(() => { if (dialogStack[dialogStack.length - 1] === dialog) (first ?? dialog).focus() })

    const onKeyDown = (event: KeyboardEvent) => {
      if (dialogStack[dialogStack.length - 1] !== dialog) return
      if (event.key === 'Escape') { event.preventDefault(); closeRef.current(); return }
      if (event.key !== 'Tab' || !dialog) return
      const items = Array.from(dialog.querySelectorAll<HTMLElement>(focusable)).filter((item) => item.checkVisibility())
      if (!items.length) { event.preventDefault(); dialog.focus(); return }
      const firstItem = items[0]
      const lastItem = items[items.length - 1]
      if (event.shiftKey && document.activeElement === firstItem) { event.preventDefault(); lastItem.focus() }
      else if (!event.shiftKey && document.activeElement === lastItem) { event.preventDefault(); firstItem.focus() }
    }

    document.addEventListener('keydown', onKeyDown)
    return () => {
      cancelAnimationFrame(focusFrame)
      document.removeEventListener('keydown', onKeyDown)
      const wasTop = dialogStack[dialogStack.length - 1] === dialog
      const index = dialogStack.indexOf(dialog)
      if (index !== -1) dialogStack.splice(index, 1)
      requestAnimationFrame(() => {
        const top = dialogStack[dialogStack.length - 1]
        if (wasTop && previous?.isConnected && (!top || top.contains(previous))) previous.focus()
      })
    }
  }, [])

  return ref
}
