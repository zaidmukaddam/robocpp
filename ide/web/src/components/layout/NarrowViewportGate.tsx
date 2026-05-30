export function NarrowViewportGate() {
  return (
    <div className="narrow-viewport-gate" role="status">
      <strong>RoboC++ Studio needs a wider screen</strong>
      <p>
        This IDE is designed for desktop workbench use. Resize the window to at least 960px wide or use a larger
        display to edit PLC projects comfortably.
      </p>
    </div>
  );
}
