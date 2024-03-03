# ap2pmsg
ap2pmsg is an Asynchronous Peer to Peer Messenger application.

The goal of this project is to allow for sending text, images, and files between devices on the local network, all the while not straying from the comfortable interface seen in popular messaging applications like Discord.

The benefits of this app being peer to peer, and completely local, are increased security and simplicity. Your sensitive data is never stored anywhere external, and you are in complete control of what the application is doing.  
ap2pmsg is asynchronous in the sense that you can send messages to hosts which are unreachable, not connected to the network. In such a case, your messages will enter a pending state, and when the recepient becomes available, they will be promptly delivered.

Another unique aspect of this project is it's architecture.  
The application is split into two components: the backend, and the frontend.  
The backend is responsible for internal operations, managing application state, and communicating with other hosts.  
The frontend's job is to present information to the user, gather user input, and issue requests to the backend.  
This decoupled approach allows for switching between many frontends while the core of the application remains the same.

Currently there is only the developement-oriented CLI frontend available, but there are plans for web and mobile frontends in the near future.

## Usage:
* Make sure you have [Rust](https://www.rust-lang.org/tools/install) installed
* [Clone the repository using git](https://docs.github.com/en/repositories/creating-and-managing-repositories/cloning-a-repository), or download it manually 
* Navigate to the project folder in your terminal of choice
* Run `$ cargo run --bin ap2pmsg`

## Contributions:
All contributions, issues, and messages are welcome! If you aren't sure about something or have any questions please reach out to me.
