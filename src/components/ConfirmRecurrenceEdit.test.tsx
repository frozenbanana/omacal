import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { ConfirmRecurrenceEdit } from "./ConfirmRecurrenceEdit";

describe("ConfirmRecurrenceEdit", () => {
  it("offers one occurrence, the entire series, and cancel", () => {
    const markup = renderToStaticMarkup(
      <ConfirmRecurrenceEdit
        title="Morning practice"
        onClose={() => {}}
        onEditSingle={() => {}}
        onEditSeries={() => {}}
      />
    );

    expect(markup).toContain("Morning practice");
    expect(markup).toContain("Only this event");
    expect(markup).toContain("Entire series");
    expect(markup).toContain("Cancel");
    expect(markup).not.toContain("future events");
  });
});
