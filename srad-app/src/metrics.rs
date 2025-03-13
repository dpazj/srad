use std::{collections::HashSet, sync::Arc};

use srad_types::{payload::{self, DataType, MetaData}, property_set::PropertySet, MetricId, MetricValue};

pub struct MetricBirthDetails {
   pub name: String,
   pub alias: Option<u64>,
   pub datatype: DataType,
}

impl MetricBirthDetails {

    fn new(name: String, alias: Option<u64>, datatype:DataType) -> Self {
        Self {
            name, 
            alias,
            datatype
        }
    }

}

pub struct MetricDetails {
    pub value: Option<MetricValue>,
    pub properties: Option<PropertySet>,
    pub metadata: Option<MetaData>,
    pub timestamp: u64,
    pub is_historical: bool,
    pub is_transient: bool
}

macro_rules! metric_details_try_from_payload_metric {
    ($metric:expr) => {{
        let timestamp = $metric.timestamp.ok_or(())?;
        
        let value = if let Some(value) = $metric.value {
            Some(value.into())
        } else if let Some(is_null) = $metric.is_null {
            if is_null { None } else { return Err(()) }
        } else { 
            return Err(()) 
        };

        let properties = match $metric.properties {
            Some(ps) => Some(ps.try_into()?),
            None => None,
        };

        let metadata = $metric.metadata;
        let is_historical = $metric.is_historical.unwrap_or(false);
        let is_transient = $metric.is_transient.unwrap_or(false);

        Ok(MetricDetails {
            value,
            properties,
            metadata,
            timestamp,
            is_historical,
            is_transient,
        })
    }}
}

pub(crate) fn get_metric_id_and_details_from_payload_metrics(metrics: Vec<payload::Metric>) -> Result<Vec<(MetricId, MetricDetails)>, ()> {
    let mut metric_id_details = Vec::with_capacity(metrics.len());
    for x in metrics {
        let id = if let Some(alias) = x.alias {
            MetricId::Alias(alias)
        } else if let Some(name) = x.name {
            MetricId::Name(name)
        } else { return Err(()) };
        let details = metric_details_try_from_payload_metric!(x)?; 
        metric_id_details.push((id, details))
    }
    Ok(metric_id_details) 
}

pub(crate) fn get_metric_birth_details_from_birth_metrics(metrics: Vec<payload::Metric>) -> Result<Vec<(MetricBirthDetails, MetricDetails)>,()> {
    //make sure metrics names and aliases are unique
    let mut metric_name_map = HashSet::with_capacity(metrics.len());
    let mut metric_alias_map = HashSet::with_capacity(metrics.len());
    let mut results= Vec::with_capacity(metrics.len());

    for x in metrics {

        let datatype = x.datatype.ok_or(())?.try_into()?;
        let name = x.name.ok_or(())?;
        if metric_name_map.insert(name.clone()) == false {
            //for now return an error, however it may be valid to provide multiple metric 
            //values for the same metric in a birth payload? seems a bit silly tho 
            return Err(())
        }

        let alias = x.alias;
        if let Some(alias) = alias {
            if metric_alias_map.insert(alias) == false {
                return Err(())
            }
        } 

        let birth_details = MetricBirthDetails::new(name, alias, datatype);
        let details = metric_details_try_from_payload_metric!(x)?;

        results.push((birth_details, details));
    }

    Ok(results) 
} 


