/** "1 трек", "3 трека", "5 треков", "11 треков", "21 трек". */
export function tracksLabel(count: number): string {
  const mod10 = count % 10;
  const mod100 = count % 100;
  let word = "треков";
  if (mod10 === 1 && mod100 !== 11) word = "трек";
  else if (mod10 >= 2 && mod10 <= 4 && (mod100 < 12 || mod100 > 14)) word = "трека";
  return `${count} ${word}`;
}
