//! Reads the Europass `Candidate` XML into plain records. Elements are matched
//! by local name, so namespace prefixes (`hr:`, `oa:`) don't matter, and
//! unknown elements are ignored (spec section 9.2). Mapping the records to a
//! CV happens in a later step.

use roxmltree::{Document, Node};
use unicode_normalization::UnicodeNormalization;

use crate::europass::EUROPASS_NS;

/// Contact and summary of the candidate.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EuropassPerson {
    pub given_name: String,
    pub family_name: String,
    pub emails: Vec<String>,
    pub phones: Vec<String>,
    pub web: Vec<String>,
    pub city: Option<String>,
    pub country: Option<String>,
    /// Plain text of `ExecutiveSummary`.
    pub summary: Option<String>,
}

/// One `PositionHistory` with its employer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EuropassPosition {
    pub organisation: String,
    pub title: String,
    pub city: Option<String>,
    pub country: Option<String>,
    /// `YYYY-MM` or `YYYY-MM-DD`, as written.
    pub start: Option<String>,
    pub end: Option<String>,
    pub current: bool,
    /// Paragraph text of the description.
    pub description: Option<String>,
    /// List items of the description.
    pub bullets: Vec<String>,
}

/// One `EducationOrganizationAttendance`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EuropassEducation {
    pub organisation: String,
    pub degree: String,
    pub city: Option<String>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub ongoing: bool,
}

/// One language with its CEFR scores, for example ("CEF-Writing-Production", "C1").
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EuropassLanguage {
    pub code: String,
    pub scores: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EuropassCv {
    pub person: EuropassPerson,
    pub positions: Vec<EuropassPosition>,
    pub education: Vec<EuropassEducation>,
    pub languages: Vec<EuropassLanguage>,
}

/// Why the XML could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XmlError {
    /// Not well-formed XML or not UTF-8.
    Malformed,
    /// Well-formed, but not a Europass `Candidate` document.
    NotEuropass,
}

/// Parses a Europass `Candidate` document.
pub fn parse_candidate(xml: &[u8]) -> Result<EuropassCv, XmlError> {
    let text = std::str::from_utf8(xml).map_err(|_| XmlError::Malformed)?;
    let doc = Document::parse(text).map_err(|_| XmlError::Malformed)?;
    let root = doc.root_element();
    if root.tag_name().name() != "Candidate" || root.tag_name().namespace() != Some(EUROPASS_NS) {
        return Err(XmlError::NotEuropass);
    }
    let mut cv = EuropassCv::default();
    if let Some(p) = child(root, "CandidatePerson") {
        cv.person = person(p);
    }
    if let Some(profile) = child(root, "CandidateProfile") {
        cv.person.summary = child(profile, "ExecutiveSummary")
            .map(|n| summary_text(&text_of(n)))
            .filter(|s| !s.is_empty());
        for employer in descendants(profile, "EmployerHistory") {
            let organisation = child_text(employer, "OrganizationName").unwrap_or_default();
            for p in children(employer, "PositionHistory") {
                cv.positions.push(position(p, &organisation));
            }
        }
        for e in descendants(profile, "EducationOrganizationAttendance") {
            cv.education.push(education(e));
        }
        for c in descendants(profile, "PersonCompetency") {
            cv.languages.push(language(c));
        }
    }
    Ok(cv)
}

fn person(p: Node) -> EuropassPerson {
    let mut out = EuropassPerson::default();
    if let Some(name) = child(p, "PersonName") {
        out.given_name = child_text(name, "GivenName").unwrap_or_default();
        out.family_name = child_text(name, "FamilyName").unwrap_or_default();
    }
    for c in children(p, "Communication") {
        if let Some(address) = child(c, "Address") {
            out.city = out.city.or_else(|| child_text(address, "CityName"));
            out.country = out.country.or_else(|| child_text(address, "CountryCode"));
            continue;
        }
        let uri = child_text(c, "URI");
        match child_text(c, "ChannelCode").as_deref() {
            Some("Email") => out.emails.extend(uri),
            Some("Web") => out.web.extend(uri),
            Some("Telephone" | "MobileTelephone") => {
                let number = child_text(c, "DialNumber");
                let phone = match (child_text(c, "CountryDialing"), number) {
                    (Some(cc), Some(n)) => Some(format!("+{} {n}", cc.trim_start_matches('+'))),
                    (None, n) => n,
                    (Some(_), None) => None,
                };
                out.phones.extend(phone.or(uri));
            }
            _ => {}
        }
    }
    out
}

fn position(p: Node, organisation: &str) -> EuropassPosition {
    let period = child(p, "EmploymentPeriod");
    let (description, bullets) = child(p, "Description")
        .map(|n| html_text(&text_of(n)))
        .unwrap_or_default();
    EuropassPosition {
        organisation: organisation.to_owned(),
        title: child_text(p, "PositionTitle").unwrap_or_default(),
        city: child_text(p, "City"),
        country: child_text(p, "Country"),
        start: period.and_then(|n| date(n, "StartDate")),
        end: period.and_then(|n| date(n, "EndDate")),
        current: period
            .and_then(|n| child_text(n, "CurrentIndicator"))
            .is_some_and(|v| v == "true"),
        description: (!description.is_empty()).then_some(description),
        bullets,
    }
}

fn education(e: Node) -> EuropassEducation {
    let period = child(e, "AttendancePeriod");
    EuropassEducation {
        organisation: child_text(e, "OrganizationName").unwrap_or_default(),
        degree: child(e, "EducationDegree")
            .and_then(|d| child_text(d, "DegreeName"))
            .unwrap_or_default(),
        city: descendants(e, "CityName").find_map(|n| non_empty(text_of(n))),
        start: period.and_then(|n| date(n, "StartDate")),
        end: period.and_then(|n| date(n, "EndDate")),
        ongoing: period
            .and_then(|n| child_text(n, "Ongoing"))
            .is_some_and(|v| v == "true"),
    }
}

fn language(c: Node) -> EuropassLanguage {
    EuropassLanguage {
        code: child_text(c, "CompetencyID").unwrap_or_default(),
        scores: children(c, "CompetencyDimension")
            .filter_map(|d| {
                let kind = child_text(d, "CompetencyDimensionTypeCode")?;
                let score = child(d, "Score").and_then(|s| child_text(s, "ScoreText"))?;
                Some((kind, score))
            })
            .collect(),
    }
}

fn date(period: Node, which: &'static str) -> Option<String> {
    child(period, which).and_then(|d| child_text(d, "FormattedDateTime"))
}

fn children<'a, 'i>(n: Node<'a, 'i>, name: &'static str) -> impl Iterator<Item = Node<'a, 'i>> {
    n.children()
        .filter(move |c| c.is_element() && c.tag_name().name() == name)
}

fn child<'a, 'i>(n: Node<'a, 'i>, name: &'static str) -> Option<Node<'a, 'i>> {
    children(n, name).next()
}

fn descendants<'a, 'i>(n: Node<'a, 'i>, name: &'static str) -> impl Iterator<Item = Node<'a, 'i>> {
    n.descendants()
        .filter(move |c| c.is_element() && c.tag_name().name() == name)
}

fn text_of(n: Node) -> String {
    n.descendants()
        .filter(Node::is_text)
        .filter_map(|t| t.text())
        .collect()
}

/// Normalizes imported text (spec section 9): NFC, control characters
/// stripped, whitespace collapsed and trimmed. Empty text becomes `None`.
fn non_empty(s: String) -> Option<String> {
    let clean: String = s
        .nfc()
        .filter(|c| !c.is_control() || c.is_whitespace())
        .collect();
    let t = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    (!t.is_empty()).then_some(t)
}

/// The summary is one text field, so its list items follow the paragraphs
/// as "- item" lines instead of being dropped.
fn summary_text(html: &str) -> String {
    let (text, items) = html_text(html);
    let items = items
        .iter()
        .map(|i| format!("- {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    [text, items]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn child_text(n: Node, name: &'static str) -> Option<String> {
    child(n, name).and_then(|c| non_empty(text_of(c)))
}

/// Splits the HTML Europass stores in descriptions into paragraph text and
/// list items. Tags are dropped, never interpreted, and common entities are
/// decoded.
pub fn html_text(html: &str) -> (String, Vec<String>) {
    let mut paragraphs = Vec::new();
    let mut bullets = Vec::new();
    let mut current = String::new();
    let mut in_item = false;
    let mut rest = html;
    let flush = |buf: &mut String, into: &mut Vec<String>| {
        if let Some(t) = non_empty(decode(buf)) {
            into.push(t);
        }
        buf.clear();
    };
    while let Some(start) = rest.find('<') {
        current.push_str(&rest[..start]);
        let Some(len) = rest[start..].find('>') else {
            rest = "";
            break;
        };
        let tag = rest[start + 1..start + len].trim().to_ascii_lowercase();
        let name = tag
            .trim_start_matches('/')
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or("");
        match name {
            "li" => {
                flush(
                    &mut current,
                    if in_item {
                        &mut bullets
                    } else {
                        &mut paragraphs
                    },
                );
                in_item = !tag.starts_with('/');
            }
            "p" | "br" | "div" | "ul" | "ol" => {
                flush(
                    &mut current,
                    if in_item {
                        &mut bullets
                    } else {
                        &mut paragraphs
                    },
                );
            }
            _ => current.push(' '),
        }
        rest = &rest[start + len + 1..];
    }
    current.push_str(rest);
    flush(
        &mut current,
        if in_item {
            &mut bullets
        } else {
            &mut paragraphs
        },
    );
    (paragraphs.join("\n\n"), bullets)
}

fn decode(s: &str) -> String {
    s.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic data in the shape of a real Europass export.
    const XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Candidate xmlns="http://www.europass.eu/1.0" xmlns:oa="http://www.openapplications.org/oagis/9" xmlns:hr="http://www.hr-xml.org/3" xmlns:eures="http://www.europass_eures.eu/1.0">
  <CandidatePerson>
    <PersonName><oa:GivenName>Jane</oa:GivenName><hr:FamilyName>Doe</hr:FamilyName></PersonName>
    <Communication><ChannelCode>Email</ChannelCode><oa:URI>jane@example.com</oa:URI></Communication>
    <Communication><ChannelCode>Telephone</ChannelCode><UseCode>mobile</UseCode><CountryDialing>49</CountryDialing><oa:DialNumber>301234567</oa:DialNumber></Communication>
    <Communication><ChannelCode>Web</ChannelCode><oa:URI>https://example.com</oa:URI></Communication>
    <Communication><UseCode>home</UseCode><Address type="home"><oa:CityName>Berlin</oa:CityName><CountryCode>de</CountryCode></Address></Communication>
    <BirthDate>1990-01-01</BirthDate>
  </CandidatePerson>
  <CandidateProfile>
    <hr:ExecutiveSummary>&lt;p&gt;Backend engineer &amp;amp; mentor.&lt;/p&gt;</hr:ExecutiveSummary>
    <EmploymentHistory>
      <EmployerHistory>
        <hr:OrganizationName>Acme GmbH</hr:OrganizationName>
        <PositionHistory>
          <PositionTitle typeCode="FREETEXT">Senior Engineer</PositionTitle>
          <eures:City>Berlin</eures:City><eures:Country>de</eures:Country>
          <EmploymentPeriod>
            <eures:StartDate><hr:FormattedDateTime>2021-03</hr:FormattedDateTime></eures:StartDate>
            <eures:CurrentIndicator>true</eures:CurrentIndicator>
          </EmploymentPeriod>
          <oa:Description>&lt;p&gt;Billing platform.&lt;/p&gt;&lt;ul&gt;&lt;li&gt;Cut latency by 43 percent.&lt;/li&gt;&lt;li&gt;Led &lt;b&gt;three&lt;/b&gt; services.&lt;/li&gt;&lt;/ul&gt;</oa:Description>
        </PositionHistory>
      </EmployerHistory>
    </EmploymentHistory>
    <EducationHistory>
      <EducationOrganizationAttendance>
        <hr:OrganizationName>TU Berlin</hr:OrganizationName>
        <OrganizationContact><Communication><Address><oa:CityName>Berlin</oa:CityName></Address></Communication></OrganizationContact>
        <AttendancePeriod>
          <StartDate><hr:FormattedDateTime>2010-10</hr:FormattedDateTime></StartDate>
          <EndDate><hr:FormattedDateTime>2014-09-30</hr:FormattedDateTime></EndDate>
          <Ongoing>false</Ongoing>
        </AttendancePeriod>
        <hr:EducationDegree><hr:DegreeName>MSc Computer Science</hr:DegreeName></hr:EducationDegree>
      </EducationOrganizationAttendance>
    </EducationHistory>
    <PersonQualifications>
      <PersonCompetency>
        <CompetencyID schemeName="NORMAL">eng</CompetencyID>
        <eures:CompetencyDimension><hr:CompetencyDimensionTypeCode>CEF-Writing-Production</hr:CompetencyDimensionTypeCode><eures:Score><hr:ScoreText>C1</hr:ScoreText></eures:Score></eures:CompetencyDimension>
      </PersonCompetency>
    </PersonQualifications>
    <SomethingNew><Unknown/></SomethingNew>
  </CandidateProfile>
  <RenderingInformation/>
</Candidate>"#;

    #[test]
    fn reads_person_positions_education_and_languages() {
        let cv = parse_candidate(XML.as_bytes()).expect("parse");
        let p = &cv.person;
        assert_eq!(
            (p.given_name.as_str(), p.family_name.as_str()),
            ("Jane", "Doe")
        );
        assert_eq!(p.emails, ["jane@example.com"]);
        assert_eq!(p.phones, ["+49 301234567"]);
        assert_eq!(p.web, ["https://example.com"]);
        assert_eq!(p.city.as_deref(), Some("Berlin"));
        assert_eq!(p.country.as_deref(), Some("de"));
        assert_eq!(p.summary.as_deref(), Some("Backend engineer & mentor."));

        let job = &cv.positions[0];
        assert_eq!(job.organisation, "Acme GmbH");
        assert_eq!(job.title, "Senior Engineer");
        assert_eq!(job.start.as_deref(), Some("2021-03"));
        assert_eq!(job.end, None);
        assert!(job.current);
        assert_eq!(job.description.as_deref(), Some("Billing platform."));
        assert_eq!(
            job.bullets,
            ["Cut latency by 43 percent.", "Led three services."]
        );

        let edu = &cv.education[0];
        assert_eq!(edu.degree, "MSc Computer Science");
        assert_eq!(edu.city.as_deref(), Some("Berlin"));
        assert_eq!(edu.end.as_deref(), Some("2014-09-30"));
        assert!(!edu.ongoing);

        assert_eq!(cv.languages[0].code, "eng");
        assert_eq!(
            cv.languages[0].scores,
            [("CEF-Writing-Production".into(), "C1".into())]
        );
    }

    #[test]
    fn refuses_malformed_and_foreign_xml() {
        assert_eq!(parse_candidate(b"<Candidate"), Err(XmlError::Malformed));
        assert_eq!(parse_candidate(&[0xff, 0xfe]), Err(XmlError::Malformed));
        assert_eq!(parse_candidate(b"<Candidate/>"), Err(XmlError::NotEuropass));
        assert_eq!(
            parse_candidate(br#"<Other xmlns="http://www.europass.eu/1.0"/>"#),
            Err(XmlError::NotEuropass)
        );
    }

    #[test]
    fn empty_candidate_gives_empty_records() {
        let cv =
            parse_candidate(br#"<Candidate xmlns="http://www.europass.eu/1.0"/>"#).expect("parse");
        assert_eq!(cv, EuropassCv::default());
    }

    #[test]
    fn html_text_splits_paragraphs_and_items_and_drops_tags() {
        let (text, items) = html_text(
            "<p>One<br/>two</p><script>x</script><ol><li>a &amp; b</li><li></li></ol>tail",
        );
        assert_eq!(text, "One\n\ntwo\n\nx\n\ntail");
        assert_eq!(items, ["a & b"]);
        assert_eq!(html_text("unclosed <b"), ("unclosed".into(), vec![]));
    }

    #[test]
    fn text_is_nfc_without_control_characters() {
        // "e" + combining acute becomes the single NFC character.
        assert_eq!(
            non_empty("Cafe\u{301}\u{7}\tbar\u{0}".into()),
            Some("Caf\u{e9} bar".into())
        );
        assert_eq!(non_empty("\u{1b}\u{7f}".into()), None);
    }

    #[test]
    fn summary_keeps_list_items() {
        assert_eq!(
            summary_text("<p>Engineer.</p><ul><li>Rust</li><li>Go</li></ul>"),
            "Engineer.\n\n- Rust\n- Go"
        );
        assert_eq!(summary_text("<ul><li>Only</li></ul>"), "- Only");
    }
}
