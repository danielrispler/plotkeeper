// Removed from ui/selection.ts in #415: `refreshCards`, which nothing imported.

// Asks the shell for the selected session's cards and draws them.
export async function refreshCards() {
  showCards(await invoke("cards"));
}
