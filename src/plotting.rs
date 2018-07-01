extern crate cpython;

use self::cpython::{Python, PyDict, PyResult, NoArgs};

pub fn test(){
    let gil = Python::acquire_gil();
    hello(gil.python()).unwrap();
}

fn hello(py: Python) -> PyResult<()> {
    let os = py.import("os")?;
    let cwd = os.call(py, "getcwd", NoArgs, None)?;
    let cwd = format!("{}{}", cwd, "/assets");
    
    let locals = PyDict::new(py);
    locals.set_item(py, "sys", py.import("sys")?)?;
    let add_assets_dir = format!("sys.path.append('{}')", cwd);
    py.eval(&add_assets_dir[..], None, Some(&locals))?;
    
    let chart = py.import("chart")?;
    chart.call(py, "show", NoArgs, None)?; 
    
    Ok(())
}
