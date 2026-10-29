const COUNTRY_CODES =
  'AD AE AF AG AI AL AM AO AQ AR AS AT AU AW AX AZ BA BB BD BE BF BG BH BI BJ BL BM BN BO BQ BR BS BT BV BW BY BZ CA CC CD CF CG CH CI CK CL CM CN CO CR CU CV CW CX CY CZ DE DJ DK DM DO DZ EC EE EG EH ER ES ET FI FJ FK FM FO FR GA GB GD GE GF GG GH GI GL GM GN GP GQ GR GS GT GU GW GY HK HM HN HR HT HU ID IE IL IM IN IO IQ IR IS IT JE JM JO JP KE KG KH KI KM KN KP KR KW KY KZ LA LB LC LI LK LR LS LT LU LV LY MA MC MD ME MF MG MH MK ML MM MN MO MP MQ MR MS MT MU MV MW MX MY MZ NA NC NE NF NG NI NL NO NP NR NU NZ OM PA PE PF PG PH PK PL PM PN PR PS PT PW PY QA RE RO RS RU RW SA SB SC SD SE SG SH SI SJ SK SL SM SN SO SR SS ST SV SX SY SZ TC TD TF TG TH TJ TK TL TM TN TO TR TT TV TW TZ UA UG UM US UY UZ VA VC VE VG VI VN VU WF WS YE YT ZA ZM ZW'.split(
    ' ',
  );

export type Option = { value: string; label: string; detail?: string };

let countries: Option[] | undefined;
let timeZones: Option[] | undefined;

export function countryOptions(): Option[] {
  if (!countries) {
    const names = new Intl.DisplayNames(['en'], { type: 'region' });
    countries = COUNTRY_CODES.map((code) => ({ value: code, label: names.of(code) ?? code, detail: code })).sort((a, b) =>
      a.label.localeCompare(b.label),
    );
  }
  return countries;
}

function offsetMinutes(zone: string, now: Date) {
  const name = new Intl.DateTimeFormat('en-US', { timeZone: zone, timeZoneName: 'longOffset' })
    .formatToParts(now)
    .find((p) => p.type === 'timeZoneName')?.value;
  const m = name?.match(/GMT([+-])(\d{2}):(\d{2})/);
  return m ? (m[1] === '-' ? -1 : 1) * (Number(m[2]) * 60 + Number(m[3])) : 0;
}

function formatOffset(minutes: number) {
  const sign = minutes < 0 ? '-' : '+';
  const abs = Math.abs(minutes);
  return `UTC${sign}${String(Math.floor(abs / 60)).padStart(2, '0')}:${String(abs % 60).padStart(2, '0')}`;
}

export function timeZoneOptions(): Option[] {
  if (!timeZones) {
    const now = new Date();
    const zones = ['UTC', ...Intl.supportedValuesOf('timeZone').filter((z) => z !== 'UTC')];
    timeZones = zones
      .map((zone) => ({ zone, offset: offsetMinutes(zone, now) }))
      .sort((a, b) => a.offset - b.offset || a.zone.localeCompare(b.zone))
      .map(({ zone, offset }) => ({ value: zone, label: zone.replaceAll('_', ' '), detail: formatOffset(offset) }));
  }
  return timeZones;
}
