//! Country and region names, as job postings write them.
//!
//! Matching is deliberately literal: a name or code must appear in the text.
//! Cities are not mapped to countries (never guess where a city is).

use std::{collections::BTreeSet, sync::LazyLock};

use regex::Regex;

/// ISO 3166-1 alpha-2 code and English name (plus XK).
pub const COUNTRIES: &[(&str, &str)] = &[
    ("AD", "Andorra"),
    ("AE", "United Arab Emirates"),
    ("AF", "Afghanistan"),
    ("AG", "Antigua & Barbuda"),
    ("AI", "Anguilla"),
    ("AL", "Albania"),
    ("AM", "Armenia"),
    ("AO", "Angola"),
    ("AQ", "Antarctica"),
    ("AR", "Argentina"),
    ("AS", "American Samoa"),
    ("AT", "Austria"),
    ("AU", "Australia"),
    ("AW", "Aruba"),
    ("AX", "Åland Islands"),
    ("AZ", "Azerbaijan"),
    ("BA", "Bosnia & Herzegovina"),
    ("BB", "Barbados"),
    ("BD", "Bangladesh"),
    ("BE", "Belgium"),
    ("BF", "Burkina Faso"),
    ("BG", "Bulgaria"),
    ("BH", "Bahrain"),
    ("BI", "Burundi"),
    ("BJ", "Benin"),
    ("BL", "St. Barthélemy"),
    ("BM", "Bermuda"),
    ("BN", "Brunei"),
    ("BO", "Bolivia"),
    ("BQ", "Caribbean Netherlands"),
    ("BR", "Brazil"),
    ("BS", "Bahamas"),
    ("BT", "Bhutan"),
    ("BV", "Bouvet Island"),
    ("BW", "Botswana"),
    ("BY", "Belarus"),
    ("BZ", "Belize"),
    ("CA", "Canada"),
    ("CC", "Cocos (Keeling) Islands"),
    ("CD", "Congo - Kinshasa"),
    ("CF", "Central African Republic"),
    ("CG", "Congo - Brazzaville"),
    ("CH", "Switzerland"),
    ("CI", "Côte d'Ivoire"),
    ("CK", "Cook Islands"),
    ("CL", "Chile"),
    ("CM", "Cameroon"),
    ("CN", "China"),
    ("CO", "Colombia"),
    ("CR", "Costa Rica"),
    ("CU", "Cuba"),
    ("CV", "Cape Verde"),
    ("CW", "Curaçao"),
    ("CX", "Christmas Island"),
    ("CY", "Cyprus"),
    ("CZ", "Czechia"),
    ("DE", "Germany"),
    ("DJ", "Djibouti"),
    ("DK", "Denmark"),
    ("DM", "Dominica"),
    ("DO", "Dominican Republic"),
    ("DZ", "Algeria"),
    ("EC", "Ecuador"),
    ("EE", "Estonia"),
    ("EG", "Egypt"),
    ("EH", "Western Sahara"),
    ("ER", "Eritrea"),
    ("ES", "Spain"),
    ("ET", "Ethiopia"),
    ("FI", "Finland"),
    ("FJ", "Fiji"),
    ("FK", "Falkland Islands"),
    ("FM", "Micronesia"),
    ("FO", "Faroe Islands"),
    ("FR", "France"),
    ("GA", "Gabon"),
    ("GB", "United Kingdom"),
    ("GD", "Grenada"),
    ("GE", "Georgia"),
    ("GF", "French Guiana"),
    ("GG", "Guernsey"),
    ("GH", "Ghana"),
    ("GI", "Gibraltar"),
    ("GL", "Greenland"),
    ("GM", "Gambia"),
    ("GN", "Guinea"),
    ("GP", "Guadeloupe"),
    ("GQ", "Equatorial Guinea"),
    ("GR", "Greece"),
    ("GS", "South Georgia & South Sandwich Islands"),
    ("GT", "Guatemala"),
    ("GU", "Guam"),
    ("GW", "Guinea-Bissau"),
    ("GY", "Guyana"),
    ("HK", "Hong Kong"),
    ("HM", "Heard & McDonald Islands"),
    ("HN", "Honduras"),
    ("HR", "Croatia"),
    ("HT", "Haiti"),
    ("HU", "Hungary"),
    ("ID", "Indonesia"),
    ("IE", "Ireland"),
    ("IL", "Israel"),
    ("IM", "Isle of Man"),
    ("IN", "India"),
    ("IO", "British Indian Ocean Territory"),
    ("IQ", "Iraq"),
    ("IR", "Iran"),
    ("IS", "Iceland"),
    ("IT", "Italy"),
    ("JE", "Jersey"),
    ("JM", "Jamaica"),
    ("JO", "Jordan"),
    ("JP", "Japan"),
    ("KE", "Kenya"),
    ("KG", "Kyrgyzstan"),
    ("KH", "Cambodia"),
    ("KI", "Kiribati"),
    ("KM", "Comoros"),
    ("KN", "St. Kitts & Nevis"),
    ("KP", "North Korea"),
    ("KR", "South Korea"),
    ("KW", "Kuwait"),
    ("KY", "Cayman Islands"),
    ("KZ", "Kazakhstan"),
    ("LA", "Laos"),
    ("LB", "Lebanon"),
    ("LC", "St. Lucia"),
    ("LI", "Liechtenstein"),
    ("LK", "Sri Lanka"),
    ("LR", "Liberia"),
    ("LS", "Lesotho"),
    ("LT", "Lithuania"),
    ("LU", "Luxembourg"),
    ("LV", "Latvia"),
    ("LY", "Libya"),
    ("MA", "Morocco"),
    ("MC", "Monaco"),
    ("MD", "Moldova"),
    ("ME", "Montenegro"),
    ("MF", "St. Martin"),
    ("MG", "Madagascar"),
    ("MH", "Marshall Islands"),
    ("MK", "North Macedonia"),
    ("ML", "Mali"),
    ("MM", "Myanmar"),
    ("MN", "Mongolia"),
    ("MO", "Macao"),
    ("MP", "Northern Mariana Islands"),
    ("MQ", "Martinique"),
    ("MR", "Mauritania"),
    ("MS", "Montserrat"),
    ("MT", "Malta"),
    ("MU", "Mauritius"),
    ("MV", "Maldives"),
    ("MW", "Malawi"),
    ("MX", "Mexico"),
    ("MY", "Malaysia"),
    ("MZ", "Mozambique"),
    ("NA", "Namibia"),
    ("NC", "New Caledonia"),
    ("NE", "Niger"),
    ("NF", "Norfolk Island"),
    ("NG", "Nigeria"),
    ("NI", "Nicaragua"),
    ("NL", "Netherlands"),
    ("NO", "Norway"),
    ("NP", "Nepal"),
    ("NR", "Nauru"),
    ("NU", "Niue"),
    ("NZ", "New Zealand"),
    ("OM", "Oman"),
    ("PA", "Panama"),
    ("PE", "Peru"),
    ("PF", "French Polynesia"),
    ("PG", "Papua New Guinea"),
    ("PH", "Philippines"),
    ("PK", "Pakistan"),
    ("PL", "Poland"),
    ("PM", "St. Pierre & Miquelon"),
    ("PN", "Pitcairn Islands"),
    ("PR", "Puerto Rico"),
    ("PS", "Palestinian Territories"),
    ("PT", "Portugal"),
    ("PW", "Palau"),
    ("PY", "Paraguay"),
    ("QA", "Qatar"),
    ("RE", "Réunion"),
    ("RO", "Romania"),
    ("RS", "Serbia"),
    ("RU", "Russia"),
    ("RW", "Rwanda"),
    ("SA", "Saudi Arabia"),
    ("SB", "Solomon Islands"),
    ("SC", "Seychelles"),
    ("SD", "Sudan"),
    ("SE", "Sweden"),
    ("SG", "Singapore"),
    ("SH", "St. Helena"),
    ("SI", "Slovenia"),
    ("SJ", "Svalbard & Jan Mayen"),
    ("SK", "Slovakia"),
    ("SL", "Sierra Leone"),
    ("SM", "San Marino"),
    ("SN", "Senegal"),
    ("SO", "Somalia"),
    ("SR", "Suriname"),
    ("SS", "South Sudan"),
    ("ST", "São Tomé & Príncipe"),
    ("SV", "El Salvador"),
    ("SX", "Sint Maarten"),
    ("SY", "Syria"),
    ("SZ", "Eswatini"),
    ("TC", "Turks & Caicos Islands"),
    ("TD", "Chad"),
    ("TF", "French Southern Territories"),
    ("TG", "Togo"),
    ("TH", "Thailand"),
    ("TJ", "Tajikistan"),
    ("TK", "Tokelau"),
    ("TL", "Timor-Leste"),
    ("TM", "Turkmenistan"),
    ("TN", "Tunisia"),
    ("TO", "Tonga"),
    ("TR", "Türkiye"),
    ("TT", "Trinidad & Tobago"),
    ("TV", "Tuvalu"),
    ("TW", "Taiwan"),
    ("TZ", "Tanzania"),
    ("UA", "Ukraine"),
    ("UG", "Uganda"),
    ("UM", "U.S. Outlying Islands"),
    ("US", "United States"),
    ("UY", "Uruguay"),
    ("UZ", "Uzbekistan"),
    ("VA", "Vatican City"),
    ("VC", "St. Vincent & Grenadines"),
    ("VE", "Venezuela"),
    ("VG", "British Virgin Islands"),
    ("VI", "U.S. Virgin Islands"),
    ("VN", "Vietnam"),
    ("VU", "Vanuatu"),
    ("WF", "Wallis & Futuna"),
    ("WS", "Samoa"),
    ("XK", "Kosovo"),
    ("YE", "Yemen"),
    ("YT", "Mayotte"),
    ("ZA", "South Africa"),
    ("ZM", "Zambia"),
    ("ZW", "Zimbabwe"),
];

/// Other ways postings name a country. Matched case-insensitively.
const ALIASES: &[(&str, &str)] = &[
    ("united states of america", "US"),
    ("america", "US"),
    ("united kingdom of great britain and northern ireland", "GB"),
    ("great britain", "GB"),
    ("britain", "GB"),
    ("england", "GB"),
    ("scotland", "GB"),
    ("wales", "GB"),
    ("northern ireland", "GB"),
    ("czech republic", "CZ"),
    ("republic of korea", "KR"),
    ("korea", "KR"),
    ("viet nam", "VN"),
    ("turkey", "TR"),
    ("turkiye", "TR"),
    ("russian federation", "RU"),
    ("holland", "NL"),
    ("the netherlands", "NL"),
    ("ivory coast", "CI"),
    ("cote d'ivoire", "CI"),
    ("côte d’ivoire", "CI"),
    ("macedonia", "MK"),
    ("bosnia and herzegovina", "BA"),
    ("bosnia", "BA"),
    ("trinidad and tobago", "TT"),
    ("antigua and barbuda", "AG"),
    ("saint kitts and nevis", "KN"),
    ("saint lucia", "LC"),
    ("saint vincent and the grenadines", "VC"),
    ("sao tome and principe", "ST"),
    ("são tomé and príncipe", "ST"),
    ("palestine", "PS"),
    ("democratic republic of the congo", "CD"),
    ("dr congo", "CD"),
    ("republic of the congo", "CG"),
    ("swaziland", "SZ"),
    ("east timor", "TL"),
    ("cabo verde", "CV"),
    ("burma", "MM"),
    ("vatican", "VA"),
    ("saudi", "SA"),
    ("emirates", "AE"),
    ("brasil", "BR"),
    ("méxico", "MX"),
    ("deutschland", "DE"),
    ("españa", "ES"),
];

/// Short codes postings use. Matched case-sensitively so "us" (the word) never counts.
const CODES: &[(&str, &str)] = &[
    ("USA", "US"),
    ("U.S.A.", "US"),
    ("U.S.A", "US"),
    ("U.S.", "US"),
    ("US", "US"),
    ("UK", "GB"),
    ("U.K.", "GB"),
    ("UAE", "AE"),
    ("KSA", "SA"),
    ("NZ", "NZ"),
    ("EU", "@EU"),
    ("EEA", "@EEA"),
    ("EMEA", "@EMEA"),
    ("LATAM", "@LATAM"),
    ("LatAm", "@LATAM"),
    ("APAC", "@APAC"),
    ("DACH", "@DACH"),
    ("CEE", "@CEE"),
    ("ANZ", "@ANZ"),
    ("AMER", "@AMERICAS"),
];

/// Region names. Matched case-insensitively.
const REGION_NAMES: &[(&str, &str)] = &[
    ("european union", "@EU"),
    ("european economic area", "@EEA"),
    ("europe", "@EUROPE"),
    ("european", "@EUROPE"),
    ("eastern europe", "@CEE"),
    ("central europe", "@CEE"),
    ("central and eastern europe", "@CEE"),
    ("western europe", "@WESTERN_EUROPE"),
    ("nordics", "@NORDICS"),
    ("nordic countries", "@NORDICS"),
    ("scandinavia", "@NORDICS"),
    ("baltics", "@BALTICS"),
    ("baltic states", "@BALTICS"),
    ("balkans", "@BALKANS"),
    ("benelux", "@BENELUX"),
    ("north america", "@NORTH_AMERICA"),
    ("south america", "@SOUTH_AMERICA"),
    ("latin america", "@LATAM"),
    ("central america", "@CENTRAL_AMERICA"),
    ("the americas", "@AMERICAS"),
    ("americas", "@AMERICAS"),
    ("caribbean", "@CARIBBEAN"),
    ("asia pacific", "@APAC"),
    ("asia-pacific", "@APAC"),
    ("asia", "@ASIA"),
    ("southeast asia", "@SOUTHEAST_ASIA"),
    ("south east asia", "@SOUTHEAST_ASIA"),
    ("south asia", "@SOUTH_ASIA"),
    ("oceania", "@ANZ"),
    ("australia and new zealand", "@ANZ"),
    ("middle east", "@MIDDLE_EAST"),
    ("mena", "@MENA"),
    ("africa", "@AFRICA"),
];

const EU: &str = "AT BE BG HR CY CZ DK EE FI FR DE GR HU IE IT LV LT LU MT NL PL PT RO SK SI ES SE";
const EEA_EXTRA: &str = "IS LI NO";
const EUROPE_EXTRA: &str = "GB CH AL AD BA BY XK MD MC ME MK RU SM RS UA VA GI IM JE GG FO AX";
const MIDDLE_EAST: &str = "AE BH CY EG IL IQ IR JO KW LB OM PS QA SA SY TR YE";
const AFRICA: &str = "DZ AO BJ BW BF BI CV CM CF TD KM CG CD CI DJ EG GQ ER SZ ET GA GM GH GN GW KE LS LR LY MG MW ML MR MU YT MA MZ NA NE NG RE RW SH ST SN SC SL SO ZA SS SD TZ TG TN UG EH ZM ZW";
const NORTH_AMERICA: &str = "US CA MX";
const CENTRAL_AMERICA: &str = "BZ CR SV GT HN NI PA";
const CARIBBEAN: &str =
    "AG AI AW BB BL BQ BS CU CW DM DO GD GP HT JM KN KY LC MF MQ MS PR SX TC TT VC VG VI";
const SOUTH_AMERICA: &str = "AR BO BR CL CO EC FK GF GY PE PY SR UY VE";
const LATAM: &str = "MX BZ CR SV GT HN NI PA AR BO BR CL CO EC PE PY UY VE CU DO PR HT";
const EAST_ASIA: &str = "CN HK MO TW JP KR KP MN";
const SOUTHEAST_ASIA: &str = "BN KH ID LA MY MM PH SG TH TL VN";
const SOUTH_ASIA: &str = "IN PK BD LK NP BT MV AF";
const CENTRAL_ASIA: &str = "KZ KG TJ TM UZ";
const OCEANIA: &str = "AU NZ PG FJ SB VU WS TO KI FM MH PW NR TV NC PF";

fn codes(lists: &[&str]) -> BTreeSet<&'static str> {
    lists
        .iter()
        .flat_map(|l| l.split(' '))
        .filter_map(|c| {
            COUNTRIES
                .iter()
                .find(|(code, _)| *code == c)
                .map(|(code, _)| *code)
        })
        .collect()
}

/// Expands a country code or `@REGION` to country codes.
fn expand(code: &str) -> BTreeSet<&'static str> {
    let europe = [EU, EEA_EXTRA, EUROPE_EXTRA];
    match code {
        "@EU" => codes(&[EU]),
        "@EEA" => codes(&[EU, EEA_EXTRA]),
        "@EUROPE" => codes(&europe),
        "@WESTERN_EUROPE" => codes(&["AT BE CH DE FR GB IE LI LU MC NL PT ES IT AD SM"]),
        "@CEE" => codes(&["BG CZ HU MD PL RO RU SK UA BY EE LV LT SI HR RS BA ME MK AL XK"]),
        "@NORDICS" => codes(&["DK FI IS NO SE FO AX"]),
        "@BALTICS" => codes(&["EE LV LT"]),
        "@BALKANS" => codes(&["AL BA BG HR XK ME MK RS SI GR RO"]),
        "@BENELUX" => codes(&["BE NL LU"]),
        "@DACH" => codes(&["DE AT CH"]),
        "@EMEA" => codes(&[EU, EEA_EXTRA, EUROPE_EXTRA, MIDDLE_EAST, AFRICA]),
        "@MIDDLE_EAST" => codes(&[MIDDLE_EAST]),
        "@MENA" => codes(&[MIDDLE_EAST, "DZ LY MA TN"]),
        "@AFRICA" => codes(&[AFRICA]),
        "@NORTH_AMERICA" => codes(&[NORTH_AMERICA]),
        "@CENTRAL_AMERICA" => codes(&[CENTRAL_AMERICA]),
        "@CARIBBEAN" => codes(&[CARIBBEAN]),
        "@SOUTH_AMERICA" => codes(&[SOUTH_AMERICA]),
        "@LATAM" => codes(&[LATAM]),
        "@AMERICAS" => codes(&[NORTH_AMERICA, CENTRAL_AMERICA, CARIBBEAN, SOUTH_AMERICA]),
        "@APAC" => codes(&[EAST_ASIA, SOUTHEAST_ASIA, SOUTH_ASIA, OCEANIA]),
        "@ASIA" => codes(&[EAST_ASIA, SOUTHEAST_ASIA, SOUTH_ASIA, CENTRAL_ASIA]),
        "@SOUTHEAST_ASIA" => codes(&[SOUTHEAST_ASIA]),
        "@SOUTH_ASIA" => codes(&[SOUTH_ASIA]),
        "@ANZ" => codes(&["AU NZ"]),
        c => codes(&[c]),
    }
}

/// US states, as in "Columbus, IN" or "Sterling Heights, Michigan".
const US_STATES: &str = "Alabama|Alaska|Arizona|Arkansas|California|Colorado|Connecticut|Delaware|Florida|Hawaii|Idaho|Illinois|Indiana|Iowa|Kansas|Kentucky|Louisiana|Maine|Maryland|Massachusetts|Michigan|Minnesota|Mississippi|Missouri|Montana|Nebraska|Nevada|New Hampshire|New Jersey|New Mexico|New York|North Carolina|North Dakota|Ohio|Oklahoma|Oregon|Pennsylvania|Rhode Island|South Carolina|South Dakota|Tennessee|Texas|Utah|Vermont|Virginia|Washington DC|West Virginia|Wisconsin|Wyoming";
const US_STATE_CODES: &str = "AL|AK|AZ|AR|CA|CO|CT|DE|FL|GA|HI|ID|IL|IN|IA|KS|KY|LA|ME|MD|MA|MI|MN|MS|MO|MT|NE|NV|NH|NJ|NM|NY|NC|ND|OH|OK|OR|PA|RI|SC|SD|TN|TX|UT|VT|VA|WA|WV|WI|WY|DC";

struct Matcher {
    names: Regex,
    codes: Regex,
    us_states: Regex,
    lookup: Vec<(String, &'static str)>,
}

static MATCHER: LazyLock<Matcher> = LazyLock::new(|| {
    let mut lookup: Vec<(String, &'static str)> = COUNTRIES
        .iter()
        .flat_map(|(code, name)| {
            let lower = name.to_lowercase();
            let mut v = vec![(lower.clone(), *code)];
            if lower.contains(" & ") {
                v.push((lower.replace(" & ", " and "), *code));
            }
            if let Some(rest) = lower.strip_prefix("st. ") {
                v.push((format!("saint {rest}"), *code));
            }
            v
        })
        .chain(ALIASES.iter().map(|(n, c)| (n.to_string(), *c)))
        .chain(REGION_NAMES.iter().map(|(n, c)| (n.to_string(), *c)))
        .collect();
    // Longest first, so "papua new guinea" wins over "guinea".
    lookup.sort_by_key(|x| std::cmp::Reverse(x.0.len()));
    let alt = |items: &mut dyn Iterator<Item = String>| items.collect::<Vec<_>>().join("|");
    let names = Regex::new(&format!(
        r"(?i)(?-u:\b)(?:{})(?-u:\b)",
        alt(&mut lookup.iter().map(|(n, _)| regex::escape(n)))
    ))
    .unwrap();
    let mut code_list: Vec<&str> = CODES.iter().map(|(c, _)| *c).collect();
    code_list.sort_by_key(|c| std::cmp::Reverse(c.len()));
    // Codes stand alone: not inside words and not followed by "-based" style letters.
    let codes = Regex::new(&format!(
        r"(?:^|[^A-Za-z.])({})(?:$|[^A-Za-z])",
        alt(&mut code_list.iter().map(|c| regex::escape(c)))
    ))
    .unwrap();
    let us_states = Regex::new(&format!(
        r"(?:(?-u:\b)(?:{US_STATES})(?-u:\b)|,\s*(?:{US_STATE_CODES})(?-u:\b))"
    ))
    .unwrap();
    Matcher {
        names,
        codes,
        us_states,
        lookup,
    }
});

/// A country or region name found in text, with its byte span.
pub struct Place {
    pub start: usize,
    pub end: usize,
    pub codes: BTreeSet<&'static str>,
}

/// Every country or region named in `text`, in order.
pub fn places(text: &str) -> Vec<Place> {
    let m = &*MATCHER;
    let mut out = Vec::new();
    for hit in m.names.find_iter(text) {
        let s = hit.as_str().to_lowercase();
        if let Some((_, code)) = m.lookup.iter().find(|(n, _)| *n == s) {
            out.push(Place {
                start: hit.start(),
                end: hit.end(),
                codes: expand(code),
            });
        }
    }
    let mut start = 0;
    while let Some(c) = m.codes.captures_at(text, start) {
        let g = c.get(1).unwrap();
        if let Some((_, code)) = CODES.iter().find(|(k, _)| *k == g.as_str()) {
            out.push(Place {
                start: g.start(),
                end: g.end(),
                codes: expand(code),
            });
        }
        start = g.end();
    }
    for hit in m.us_states.find_iter(text) {
        out.push(Place {
            start: hit.start(),
            end: hit.end(),
            codes: BTreeSet::from(["US"]),
        });
    }
    out.sort_by_key(|p| p.start);
    out
}

/// Countries and regions named in `text`, as country codes.
pub fn countries_in(text: &str) -> BTreeSet<&'static str> {
    places(text).into_iter().flat_map(|p| p.codes).collect()
}

/// English name for a country code, or the code itself.
pub fn country_name(code: &str) -> &str {
    COUNTRIES
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, n)| *n)
        .unwrap_or(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(codes: &[&'static str]) -> BTreeSet<&'static str> {
        codes.iter().copied().collect()
    }

    #[test]
    fn finds_countries_and_codes() {
        assert_eq!(countries_in("USA, Canada"), set(&["CA", "US"]));
        assert_eq!(countries_in("Remote - US"), set(&["US"]));
        assert_eq!(countries_in("UK, EU").len(), 28);
        assert_eq!(countries_in("🇨🇦 Canada and 🇫🇷 France"), set(&["CA", "FR"]));
        assert_eq!(
            countries_in("Bangalore, India; Remote, Australia; Remote, Israel"),
            set(&["AU", "IL", "IN"])
        );
    }

    #[test]
    fn ignores_lowercase_us_and_cities() {
        assert!(countries_in("join us, we are remote").is_empty());
        assert!(countries_in("Copenhagen").is_empty());
        assert!(countries_in("Remote").is_empty());
    }

    #[test]
    fn expands_regions() {
        let europe = countries_in("Europe");
        assert!(europe.contains("PT") && europe.contains("GB") && !europe.contains("US"));
        assert!(countries_in("EMEA").contains("ZA"));
        assert!(countries_in("LATAM").contains("BR"));
        assert!(countries_in("APAC").contains("SG"));
    }

    #[test]
    fn prefers_longer_names() {
        assert_eq!(countries_in("Papua New Guinea"), set(&["PG"]));
        assert_eq!(countries_in("Northern Ireland"), set(&["GB"]));
        assert_eq!(
            countries_in("United Kingdom of Great Britain and Northern Ireland"),
            set(&["GB"])
        );
    }

    #[test]
    fn reads_us_states() {
        assert_eq!(countries_in("SIHO - Columbus, IN"), set(&["US"]));
        assert_eq!(countries_in("Sterling Heights, Michigan"), set(&["US"]));
        assert!(countries_in("Vancouver, BC, Canada") == set(&["CA"]));
    }
}
