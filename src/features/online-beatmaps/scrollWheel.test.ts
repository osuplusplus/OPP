import { expect, it } from "vitest";
import { scrollWheel } from "./scrollWheel";

function area(height: number, content: number, top = 0) {
  const node = document.createElement("div");
  Object.defineProperties(node, { clientHeight: { value: height }, scrollHeight: { value: content } });
  node.scrollTop = top;
  return node;
}

it("scrolls the floating details first and passes only overflow to their original list", () => {
  const popup = area(240, 600, 300), list = area(500, 2000, 100);
  const event = new WheelEvent("wheel", { deltaY: 100, cancelable: true });
  scrollWheel(event, popup, list);
  expect(popup.scrollTop).toBe(360);
  expect(list.scrollTop).toBe(140);
  expect(event.defaultPrevented).toBe(true);
  scrollWheel(new WheelEvent("wheel", { deltaY: -400 }), popup, list);
  expect(popup.scrollTop).toBe(0);
  expect(list.scrollTop).toBe(100);
});

it("lets the list scroll beneath short details and leaves zoom and horizontal gestures alone", () => {
  const popup = area(180, 180), list = area(500, 2000);
  scrollWheel(new WheelEvent("wheel", { deltaY: 3, deltaMode: 1 }), popup, list);
  expect(list.scrollTop).toBe(48);
  const zoom = new WheelEvent("wheel", { deltaY: 80, ctrlKey: true, cancelable: true });
  scrollWheel(zoom, popup, list);
  expect(zoom.defaultPrevented).toBe(false);
  scrollWheel(new WheelEvent("wheel", { deltaY: 10, deltaX: 100 }), popup, list);
  expect(list.scrollTop).toBe(48);
});
