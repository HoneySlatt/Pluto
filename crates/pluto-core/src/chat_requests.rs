//! Pluto's chat requests (not upstream): the chat composer's Research and
//! Image toggles lead the message with a context record the server hands to
//! the agent as it is. The desktop's `chat_research` and `chat_image` and the
//! mobile app send the same records.

use serde_json::{Value, json};

use crate::context::format_reference;

pub const RESEARCH_LABEL: &str = "Research";

/// What the agent is asked to do with a research message.
const RESEARCH_REQUEST: &str = "Deep research requested: treat the user's message as a research question and investigate it in depth before answering.
1. If the request is ambiguous in a way that would change the research, ask up to three short clarifying questions and stop there. Otherwise state your understanding in one line and start.
2. Plan the sub-questions. Search the web broadly, with many queries from several angles, favouring primary and recent sources, and read the most relevant pages in full rather than relying on snippets.
3. Cross-check important facts across independent sources, and note disagreements and how recent each source is.
4. Keep going until the key questions are answered or new searches stop adding anything.
5. Write a structured report in the user's language: a short summary of the findings first, then sections with headings, tables where they help compare, and a last section on open questions and limits. Cite every factual claim inline with Markdown links to the pages you used, and end with the list of sources. When a chart makes the findings clearer and html_render is available, publish one.";

/// The text and context records of a research message: `text` led by the
/// research chip, and `records` with its record.
pub fn research_message(text: &str, records: &[Value]) -> (String, Vec<Value>) {
    let id = format!("research_{}", uuid::Uuid::new_v4().simple());
    let record = json!({
        "version": 1,
        "contextId": id,
        "kind": "research",
        "label": RESEARCH_LABEL,
        "payload": {"request": RESEARCH_REQUEST},
    });
    let chip = format_reference("research", &id, RESEARCH_LABEL);
    let mut records = records.to_vec();
    records.push(record);
    (format!("{chip} {text}"), records)
}

pub const IMAGE_LABEL: &str = "Image";

const IMAGE_REQUEST: &str = "Image requested: create the image the user describes with your image generation tool, not with code, SVG or HTML. If the description is too vague to make something good, ask one short question first. After generating, reply in one short sentence in the user's language.";

/// The text and context records of an image message: `text` led by the
/// Image chip, and `records` with its record.
pub fn image_message(text: &str, records: &[Value]) -> (String, Vec<Value>) {
    let id = format!("image_{}", uuid::Uuid::new_v4().simple());
    let record = json!({
        "version": 1,
        "contextId": id,
        "kind": "image-request",
        "label": IMAGE_LABEL,
        "payload": {"request": IMAGE_REQUEST},
    });
    let chip = format_reference("image-request", &id, IMAGE_LABEL);
    let mut records = records.to_vec();
    records.push(record);
    (format!("{chip} {text}"), records)
}
