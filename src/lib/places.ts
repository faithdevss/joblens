// Country, timezone and currency options for search settings.

// ISO 3166-1 alpha-2 codes (plus XK). Names come from Intl so they stay correct.
const COUNTRY_CODES =
  "AD AE AF AG AI AL AM AO AQ AR AS AT AU AW AX AZ BA BB BD BE BF BG BH BI BJ BL BM BN BO BQ BR BS BT BV BW BY BZ CA CC CD CF CG CH CI CK CL CM CN CO CR CU CV CW CX CY CZ DE DJ DK DM DO DZ EC EE EG EH ER ES ET FI FJ FK FM FO FR GA GB GD GE GF GG GH GI GL GM GN GP GQ GR GS GT GU GW GY HK HM HN HR HT HU ID IE IL IM IN IO IQ IR IS IT JE JM JO JP KE KG KH KI KM KN KP KR KW KY KZ LA LB LC LI LK LR LS LT LU LV LY MA MC MD ME MF MG MH MK ML MM MN MO MP MQ MR MS MT MU MV MW MX MY MZ NA NC NE NF NG NI NL NO NP NR NU NZ OM PA PE PF PG PH PK PL PM PN PR PS PT PW PY QA RE RO RS RU RW SA SB SC SD SE SG SH SI SJ SK SL SM SN SO SR SS ST SV SX SY SZ TC TD TF TG TH TJ TK TL TM TN TO TR TT TV TW TZ UA UG UM US UY UZ VA VC VE VG VI VN VU WF WS XK YE YT ZA ZM ZW".split(
    " ",
  );

const regionNames = new Intl.DisplayNames(["en"], { type: "region" });

export const COUNTRIES: ReadonlyArray<{ code: string; name: string }> = COUNTRY_CODES.map(
  (code) => ({ code, name: regionNames.of(code) ?? code }),
).sort((a, b) => a.name.localeCompare(b.name));

// Common alternative names people write on resumes.
const ALIASES: Record<string, string> = {
  usa: "US",
  "u.s.": "US",
  "united states of america": "US",
  uk: "GB",
  england: "GB",
  scotland: "GB",
  wales: "GB",
  uae: "AE",
  holland: "NL",
};

/** Best-effort country code from a location like "Lisbon, Portugal". */
export function countryFromLocation(location: string): string {
  const parts = location.split(/[,·|]/).map((p) => p.trim().toLowerCase());
  for (const part of parts.reverse()) {
    if (ALIASES[part]) return ALIASES[part];
    const hit = COUNTRIES.find((c) => c.name.toLowerCase() === part);
    if (hit) return hit.code;
  }
  return "";
}

const OFFSETS = [
  "-12:00",
  "-11:00",
  "-10:00",
  "-09:30",
  "-09:00",
  "-08:00",
  "-07:00",
  "-06:00",
  "-05:00",
  "-04:00",
  "-03:30",
  "-03:00",
  "-02:00",
  "-01:00",
  "+00:00",
  "+01:00",
  "+02:00",
  "+03:00",
  "+03:30",
  "+04:00",
  "+04:30",
  "+05:00",
  "+05:30",
  "+05:45",
  "+06:00",
  "+06:30",
  "+07:00",
  "+08:00",
  "+08:45",
  "+09:00",
  "+09:30",
  "+10:00",
  "+10:30",
  "+11:00",
  "+12:00",
  "+12:45",
  "+13:00",
  "+14:00",
];

export const TIMEZONES: ReadonlyArray<{ value: string; label: string }> = OFFSETS.map((o) => ({
  value: `UTC${o}`,
  label: `UTC${o.replace("-", "−")}`,
}));

/** This computer's current UTC offset, e.g. "UTC+05:30". */
export function localTimezone(date = new Date()): string {
  const total = -date.getTimezoneOffset();
  const sign = total < 0 ? "-" : "+";
  const abs = Math.abs(total);
  const hh = String(Math.floor(abs / 60)).padStart(2, "0");
  const mm = String(abs % 60).padStart(2, "0");
  return `UTC${sign}${hh}:${mm}`;
}

export const CURRENCIES = [
  "USD",
  "EUR",
  "GBP",
  "CAD",
  "AUD",
  "NZD",
  "CHF",
  "SEK",
  "NOK",
  "DKK",
  "PLN",
  "CZK",
  "INR",
  "PKR",
  "BRL",
  "MXN",
  "ARS",
  "ZAR",
  "SGD",
  "JPY",
];
